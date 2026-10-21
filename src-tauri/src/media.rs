//! Now Playing: what the panel shows is playing. See CONTRACT.md section 7.

use std::any::Any;
use std::io::{BufRead, BufReader};
use std::panic::AssertUnwindSafe;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Track {
    pub title: String,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub app: String,
    pub bundle_id: String,
    pub playing: bool,
    pub duration_ms: Option<u64>,
    pub elapsed_ms: Option<u64>,
    pub elapsed_at_ms: Option<u64>,
    pub artwork: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Via {
    Adapter,
    Applescript,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Health {
    Ok,
    Denied,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Media {
    pub via: Via,
    pub health: Health,
    pub track: Option<Track>,
}

pub fn app_name(bundle_id: &str, parent: Option<&str>) -> String {
    let id = parent.unwrap_or(bundle_id);
    let name = match id {
        "com.apple.Music" => "Apple Music",
        "com.spotify.client" => "Spotify",
        "com.google.Chrome" => "Chrome",
        "com.apple.Safari" => "Safari",
        "company.thebrowser.Browser" => "Arc",
        "org.mozilla.firefox" => "Firefox",
        "com.microsoft.edgemac" => "Edge",
        "com.brave.Browser" => "Brave",
        "com.apple.podcasts" => "Podcasts",
        "com.apple.QuickTimePlayerX" => "QuickTime Player",
        other => other,
    };
    name.to_string()
}

fn non_empty_str<'a>(payload: &'a Value, key: &str) -> Option<&'a str> {
    payload.get(key)?.as_str().filter(|s| !s.is_empty())
}

fn micros_to_ms(payload: &Value, key: &str) -> Option<u64> {
    let v = payload.get(key)?;
    let micros = v.as_u64().or_else(|| v.as_f64().filter(|f| *f >= 0.0).map(|f| f as u64))?;
    Some(micros / 1000)
}

pub fn parse_adapter_line(line: &str) -> Result<Option<Track>, String> {
    let v: Value = serde_json::from_str(line).map_err(|e| format!("invalid JSON: {e}"))?;
    match v.get("type").and_then(Value::as_str) {
        Some("data") => {}
        other => return Err(format!("unexpected type {other:?}")),
    }
    if v.get("diff").and_then(Value::as_bool) == Some(true) {
        return Err("diff lines are not expected".into());
    }
    let Some(payload) = v.get("payload") else { return Ok(None) };
    let Some(title) = non_empty_str(payload, "title") else { return Ok(None) };
    let bundle_id = payload.get("bundleIdentifier").and_then(Value::as_str).unwrap_or("");
    let parent = payload.get("parentApplicationBundleIdentifier").and_then(Value::as_str);
    let artwork = match (non_empty_str(payload, "artworkMimeType"), non_empty_str(payload, "artworkData")) {
        (Some(mime), Some(data)) => Some(format!("data:{mime};base64,{data}")),
        _ => None,
    };
    Ok(Some(Track {
        title: title.to_string(),
        artist: non_empty_str(payload, "artist").map(str::to_string),
        album: non_empty_str(payload, "album").map(str::to_string),
        app: app_name(bundle_id, parent),
        bundle_id: bundle_id.to_string(),
        playing: payload.get("playing").and_then(Value::as_bool).unwrap_or(false),
        duration_ms: micros_to_ms(payload, "durationMicros"),
        elapsed_ms: micros_to_ms(payload, "elapsedTimeMicros"),
        elapsed_at_ms: micros_to_ms(payload, "timestampEpochMicros"),
        artwork,
    }))
}

fn seconds_to_ms(s: &str) -> Option<u64> {
    let secs: f64 = s.trim().parse().ok()?;
    (secs.is_finite() && secs >= 0.0).then(|| (secs * 1000.0).round() as u64)
}

fn none_if_empty(s: &str) -> Option<String> {
    (!s.is_empty()).then(|| s.to_string())
}

/// One `<music|spotify>\t<state>\t<title>\t<artist>\t<album>\t<duration s>\t<position s>\t<artwork>` line.
fn parse_applescript_line(line: &str, now_ms: u64) -> Option<Track> {
    let f: Vec<&str> = line.split('\t').collect();
    if f.len() < 8 {
        return None;
    }
    let (bundle_id, app) = match f[0] {
        "music" => ("com.apple.Music", "Apple Music"),
        "spotify" => ("com.spotify.client", "Spotify"),
        _ => return None,
    };
    let playing = match f[1] {
        "playing" => true,
        "paused" => false,
        _ => return None,
    };
    if f[2].is_empty() {
        return None;
    }
    Some(Track {
        title: f[2].to_string(),
        artist: none_if_empty(f[3]),
        album: none_if_empty(f[4]),
        app: app.to_string(),
        bundle_id: bundle_id.to_string(),
        playing,
        duration_ms: Some(seconds_to_ms(f[5])?),
        elapsed_ms: Some(seconds_to_ms(f[6])?),
        elapsed_at_ms: Some(now_ms),
        artwork: none_if_empty(f[7].trim_end_matches('\r')),
    })
}

pub fn parse_applescript(out: &str, now_ms: u64) -> Option<Track> {
    let tracks: Vec<Track> = out.lines().filter_map(|l| parse_applescript_line(l, now_ms)).collect();
    let first_playing = tracks.iter().position(|t| t.playing);
    let pick = first_playing.or_else(|| (!tracks.is_empty()).then_some(0))?;
    tracks.into_iter().nth(pick)
}

pub fn applescript_denied(stderr: &str) -> bool {
    stderr.contains("-1743")
}

pub fn backoff(attempt: u32) -> Duration {
    Duration::from_secs(2u64.checked_pow(attempt).map_or(60, |s| s.min(60)))
}

// ---- Provider: processes and timing. Everything above is pure. ----

const ADAPTER_TEST_TIMEOUT: Duration = Duration::from_secs(5);
const STREAM_HEALTHY_AFTER: Duration = Duration::from_secs(60);
const APPLESCRIPT_POLL: Duration = Duration::from_secs(2);
const ADAPTER_RETEST: Duration = Duration::from_secs(60);

const MUSIC_SCRIPT: &str = r#"tell application "Music"
set s to player state as text
if s is "stopped" then return "music" & tab & s & tab & tab & tab & tab & "0" & tab & "0" & tab
set t to current track
return "music" & tab & s & tab & (name of t) & tab & (artist of t) & tab & (album of t) & tab & ((round (duration of t)) as text) & tab & ((round (player position)) as text) & tab
end tell"#;

const SPOTIFY_SCRIPT: &str = r#"tell application "Spotify"
set s to player state as text
if s is "stopped" then return "spotify" & tab & s & tab & tab & tab & tab & "0" & tab & "0" & tab
set t to current track
return "spotify" & tab & s & tab & (name of t) & tab & (artist of t) & tab & (album of t) & tab & ((round ((duration of t) / 1000)) as text) & tab & ((round (player position)) as text) & tab & (artwork url of t)
end tell"#;

/// `$NOTCH_MEDIA_ADAPTER` when set and non-empty, else `target/adapter` next to `target/<profile>/notch-hud`.
pub fn adapter_dir() -> PathBuf {
    match std::env::var_os("NOTCH_MEDIA_ADAPTER") {
        Some(dir) if !dir.is_empty() => PathBuf::from(dir),
        _ => std::env::current_exe()
            .ok()
            .and_then(|exe| Some(exe.parent()?.parent()?.join("adapter")))
            .unwrap_or_else(|| PathBuf::from("adapter")),
    }
}

fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64)
}

fn adapter_command(dir: &Path, args: &[&str]) -> Command {
    let mut cmd = Command::new("/usr/bin/perl");
    cmd.arg(dir.join("mediaremote-adapter.pl"))
        .arg(dir.join("MediaRemoteAdapter.framework"))
        .arg(dir.join("MediaRemoteAdapterTestClient"))
        .args(args)
        .stdin(Stdio::null());
    cmd
}

/// Kills the child when dropped, also while unwinding from a panic, so no adapter outlives its reader.
struct Reaped(Child);

impl Drop for Reaped {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn test_adapter(dir: &Path) -> Result<(), String> {
    let script = dir.join("mediaremote-adapter.pl");
    if !script.is_file() {
        return Err(format!("{} not found", script.display()));
    }
    let child = adapter_command(dir, &["test"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("test did not start: {e}"))?;
    let mut child = Reaped(child);
    let started = Instant::now();
    loop {
        match child.0.try_wait() {
            Ok(Some(status)) if status.success() => return Ok(()),
            Ok(Some(status)) => return Err(format!("test {status}")),
            Ok(None) if started.elapsed() >= ADAPTER_TEST_TIMEOUT => {
                return Err(format!("test timed out after {} s", ADAPTER_TEST_TIMEOUT.as_secs()))
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(50)),
            Err(e) => return Err(format!("test wait: {e}")),
        }
    }
}

/// Feeds every stream line to `sink` until the adapter exits.
fn stream_adapter(dir: &Path, sink: &dyn Fn(Media)) {
    let child = adapter_command(dir, &["stream", "--micros", "--no-diff", "--debounce=100"])
        .stdout(Stdio::piped())
        .spawn();
    let mut child = match child {
        Ok(child) => Reaped(child),
        Err(e) => return eprintln!("media: adapter stream did not start: {e}"),
    };
    let Some(stdout) = child.0.stdout.take() else { return };
    for line in BufReader::new(stdout).lines() {
        let line = match line {
            Ok(line) => line,
            Err(e) => {
                eprintln!("media: adapter stream read: {e}");
                break;
            }
        };
        match parse_adapter_line(&line) {
            Ok(track) => sink(Media { via: Via::Adapter, health: Health::Ok, track }),
            Err(e) => eprintln!("media: skipped adapter line: {e}"),
        }
    }
    match child.0.wait() {
        Ok(status) => eprintln!("media: adapter stream ended ({status})"),
        Err(e) => eprintln!("media: adapter stream wait: {e}"),
    }
}

fn running(process: &str) -> bool {
    Command::new("/usr/bin/pgrep")
        .args(["-x", process])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

/// Asks only apps that are already running, so polling never launches one.
fn poll_applescript() -> Media {
    let mut out = String::new();
    let mut denied = false;
    for (process, script) in [("Music", MUSIC_SCRIPT), ("Spotify", SPOTIFY_SCRIPT)] {
        if !running(process) {
            continue;
        }
        match Command::new("/usr/bin/osascript").args(["-e", script]).stdin(Stdio::null()).output() {
            Ok(o) => {
                denied |= applescript_denied(&String::from_utf8_lossy(&o.stderr));
                out.push_str(&String::from_utf8_lossy(&o.stdout));
                out.push('\n');
            }
            Err(e) => {
                eprintln!("media: osascript did not start: {e}");
                return Media { via: Via::Applescript, health: Health::Failed, track: None };
            }
        }
    }
    match parse_applescript(&out, now_ms()) {
        Some(track) => Media { via: Via::Applescript, health: Health::Ok, track: Some(track) },
        None if denied => Media { via: Via::Applescript, health: Health::Denied, track: None },
        None => Media { via: Via::Applescript, health: Health::Ok, track: None },
    }
}

struct Provider {
    dir: PathBuf,
    attempt: u32,
    via: Via,
}

impl Provider {
    fn back_off(&mut self) {
        std::thread::sleep(backoff(self.attempt));
        self.attempt = self.attempt.saturating_add(1);
    }

    /// One pass: stream the adapter until it exits, or poll AppleScript until the next adapter test.
    fn step(&mut self, sink: &dyn Fn(Media)) {
        match test_adapter(&self.dir) {
            Ok(()) => {
                self.via = Via::Adapter;
                let started = Instant::now();
                stream_adapter(&self.dir, sink);
                if started.elapsed() >= STREAM_HEALTHY_AFTER {
                    self.attempt = 0;
                }
                self.back_off();
            }
            Err(reason) => {
                if self.via != Via::Applescript {
                    eprintln!("media: adapter Failed ({reason}), using AppleScript");
                    self.via = Via::Applescript;
                }
                let started = Instant::now();
                while started.elapsed() < ADAPTER_RETEST {
                    sink(poll_applescript());
                    std::thread::sleep(APPLESCRIPT_POLL);
                }
            }
        }
    }
}

fn panic_reason(panic: &(dyn Any + Send)) -> String {
    panic
        .downcast_ref::<&str>()
        .map(|s| s.to_string())
        .or_else(|| panic.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "unknown panic".into())
}

/// Never returns. Reports every reading to `sink`; a panic is logged, reported as `Failed`, and retried.
pub fn run(sink: impl Fn(Media) + Send + Sync + 'static) -> ! {
    let mut provider = Provider { dir: adapter_dir(), attempt: 0, via: Via::None };
    loop {
        let pass = std::panic::catch_unwind(AssertUnwindSafe(|| provider.step(&sink)));
        if let Err(panic) = pass {
            eprintln!("media: Failed: {}", panic_reason(panic.as_ref()));
            sink(Media { via: provider.via, health: Health::Failed, track: None });
            provider.back_off();
        }
    }
}

#[cfg(test)]
#[path = "media_tests.rs"]
mod tests;
