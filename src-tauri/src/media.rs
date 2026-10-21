//! Now Playing: what the panel shows is playing. See CONTRACT.md section 7.

use std::any::Any;
use std::io::{BufRead, BufReader};
use std::panic::AssertUnwindSafe;
use std::path::{Path, PathBuf};
use std::os::unix::process::CommandExt;
use std::process::{Child, Command, ExitStatus, Output, Stdio};
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

fn parse_ms(s: &str) -> Option<u64> {
    let ms: f64 = s.trim().parse().ok()?;
    (ms.is_finite() && ms >= 0.0).then(|| ms.round() as u64)
}

fn none_if_empty(s: &str) -> Option<String> {
    (!s.is_empty()).then(|| s.to_string())
}

/// One `<music|spotify>\t<state>\t<title>\t<artist>\t<album>\t<duration ms>\t<position ms>\t<artwork>` line.
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
        duration_ms: Some(parse_ms(f[5])?),
        elapsed_ms: Some(parse_ms(f[6])?),
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
const OSASCRIPT_TIMEOUT: Duration = Duration::from_secs(3);
const STREAM_HEALTHY_AFTER: Duration = Duration::from_secs(60);
const APPLESCRIPT_POLL: Duration = Duration::from_secs(2);
const ADAPTER_RETEST: Duration = Duration::from_secs(60);

const MUSIC_SCRIPT: &str = r#"tell application "Music"
set s to player state as text
if s is "stopped" then return "music" & tab & s & tab & tab & tab & tab & "0" & tab & "0" & tab
set t to current track
return "music" & tab & s & tab & (name of t) & tab & (artist of t) & tab & (album of t) & tab & ((round ((duration of t) * 1000)) as text) & tab & ((round ((player position) * 1000)) as text) & tab
end tell"#;

const SPOTIFY_SCRIPT: &str = r#"tell application "Spotify"
set s to player state as text
if s is "stopped" then return "spotify" & tab & s & tab & tab & tab & tab & "0" & tab & "0" & tab
set t to current track
return "spotify" & tab & s & tab & (name of t) & tab & (artist of t) & tab & (album of t) & tab & ((duration of t) as text) & tab & ((round ((player position) * 1000)) as text) & tab & (artwork url of t)
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

extern "C" {
    fn kill(pid: i32, sig: i32) -> i32;
}

const SIGKILL: i32 = 9;

/// Sends `signal` to every process in group `pgid`.
fn signal_group(pgid: u32, signal: i32) {
    let Ok(pgid) = i32::try_from(pgid) else { return };
    // SAFETY: kill(2) only sends a signal; it reads and writes no memory of ours.
    unsafe { kill(-pgid, signal) };
}

/// The one stop policy (CONTRACT.md section 7). Runs beside an adapter group with the app holding its stdin; EOF
/// means stop, whether the app closed it or is gone, SIGKILL included. Lists the adapter's direct children first
/// (its test client runs in a group of its own), sends SIGTERM to the group and to them, waits up to 5 s, then sends
/// SIGKILL to whatever of them is left. `$1` is the group id, which is also the adapter's pid.
const WATCHDOG: &str = r#"read _
g="$1"
kids="$(/usr/bin/pgrep -P "$g")"
alive() {
  kill -s 0 -- "-$g" 2>/dev/null && return 0
  for k in $kids; do kill -s 0 "$k" 2>/dev/null && return 0; done
  return 1
}
kill -s TERM -- "-$g" 2>/dev/null
for k in $kids; do kill -s TERM "$k" 2>/dev/null; done
i=0
while [ "$i" -lt 50 ] && alive; do sleep 0.1; i=$((i + 1)); done
if alive; then
  kill -s KILL -- "-$g" 2>/dev/null
  for k in $kids; do kill -s KILL "$k" 2>/dev/null; done
fi"#;

/// An adapter process leading its own process group, plus the watchdog that stops it when its stdin closes.
/// Dropping it stops the adapter, also while unwinding from a panic, so no adapter outlives its reader.
struct Group {
    child: Child,
    watchdog: Child,
    stopped: bool,
}

impl Group {
    fn spawn(mut cmd: Command) -> Result<Group, String> {
        let mut child = cmd.process_group(0).spawn().map_err(|e| e.to_string())?;
        let pgid = child.id();
        let watchdog = Command::new("/bin/sh")
            .args(["-c", WATCHDOG, "notch-hud-watchdog", &pgid.to_string()])
            .process_group(0)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn();
        match watchdog {
            Ok(watchdog) => Ok(Group { child, watchdog, stopped: false }),
            Err(e) => {
                signal_group(pgid, SIGKILL);
                let _ = child.wait();
                Err(format!("watchdog did not start: {e}"))
            }
        }
    }

    /// Closes the watchdog's stdin so it runs the stop policy, then reaps the adapter and the watchdog. The adapter
    /// is reaped while the watchdog probes, so a zombie leader does not keep the group alive for the whole wait.
    fn stop(&mut self) {
        if std::mem::replace(&mut self.stopped, true) {
            return;
        }
        drop(self.watchdog.stdin.take());
        loop {
            let _ = self.child.try_wait();
            match self.watchdog.try_wait() {
                Ok(None) => std::thread::sleep(Duration::from_millis(50)),
                Ok(Some(_)) | Err(_) => break,
            }
        }
        // Only when the watchdog failed to end it; the adapter is our unreaped child, so its pid is still its own.
        if let Ok(None) = self.child.try_wait() {
            let _ = self.child.kill();
        }
        let _ = self.child.wait();
    }
}

impl Drop for Group {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Waits for `child` until `timeout` has passed. `Ok(None)` on timeout; stopping it is the caller's job.
fn wait_deadline(child: &mut Child, timeout: Duration) -> Result<Option<ExitStatus>, String> {
    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Ok(Some(status)),
            Ok(None) if started.elapsed() >= timeout => return Ok(None),
            Ok(None) => std::thread::sleep(Duration::from_millis(50)),
            Err(e) => return Err(e.to_string()),
        }
    }
}

fn test_adapter(dir: &Path) -> Result<(), String> {
    let script = dir.join("mediaremote-adapter.pl");
    if !script.is_file() {
        return Err(format!("{} not found", script.display()));
    }
    let mut cmd = adapter_command(dir, &["test"]);
    cmd.stdout(Stdio::null()).stderr(Stdio::null());
    let mut group = Group::spawn(cmd).map_err(|e| format!("test did not start: {e}"))?;
    match wait_deadline(&mut group.child, ADAPTER_TEST_TIMEOUT) {
        Ok(Some(status)) if status.success() => Ok(()),
        Ok(Some(status)) => Err(format!("test {status}")),
        Ok(None) => Err(format!("test timed out after {} s", ADAPTER_TEST_TIMEOUT.as_secs())),
        Err(e) => Err(format!("test wait: {e}")),
    }
}

/// Feeds every stream line to `sink` until the adapter exits.
fn stream_adapter(dir: &Path, sink: &dyn Fn(Media)) {
    let mut cmd = adapter_command(dir, &["stream", "--micros", "--no-diff", "--debounce=100"]);
    cmd.stdout(Stdio::piped());
    let mut group = match Group::spawn(cmd) {
        Ok(group) => group,
        Err(e) => return eprintln!("media: adapter stream did not start: {e}"),
    };
    let Some(stdout) = group.child.stdout.take() else { return };
    for line in BufReader::new(stdout).lines() {
        let line = match line {
            Ok(line) => line,
            Err(e) => {
                eprintln!("media: adapter stream read: {e}");
                group.stop();
                break;
            }
        };
        match parse_adapter_line(&line) {
            Ok(track) => sink(Media { via: Via::Adapter, health: Health::Ok, track }),
            Err(e) => eprintln!("media: skipped adapter line: {e}"),
        }
    }
    match group.child.wait() {
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

/// Runs one script, killed after OSASCRIPT_TIMEOUT. Its output is one short line, so the pipes never fill up.
fn osascript(script: &str) -> Result<Output, String> {
    let mut child = Command::new("/usr/bin/osascript")
        .args(["-e", script])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("did not start: {e}"))?;
    let failure = match wait_deadline(&mut child, OSASCRIPT_TIMEOUT) {
        Ok(Some(_)) => return child.wait_with_output().map_err(|e| format!("output: {e}")),
        Ok(None) => format!("timed out after {} s", OSASCRIPT_TIMEOUT.as_secs()),
        Err(e) => format!("wait: {e}"),
    };
    let _ = child.kill();
    let _ = child.wait();
    Err(failure)
}

/// Asks only apps that are already running, so polling never launches one.
fn poll_applescript() -> Media {
    let mut out = String::new();
    let mut denied = false;
    for (process, script) in [("Music", MUSIC_SCRIPT), ("Spotify", SPOTIFY_SCRIPT)] {
        if !running(process) {
            continue;
        }
        match osascript(script) {
            Ok(o) => {
                denied |= applescript_denied(&String::from_utf8_lossy(&o.stderr));
                out.push_str(&String::from_utf8_lossy(&o.stdout));
                out.push('\n');
            }
            Err(e) => {
                eprintln!("media: osascript {e}");
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

// ---- Controls (CONTRACT.md section 7, "Controls") ----

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase", deny_unknown_fields)]
pub enum Action {
    Toggle,
    Next,
    Previous,
    Seek { position_ms: u64 },
}

impl Action {
    /// The word the trace lines use.
    pub fn label(self) -> String {
        match self {
            Action::Toggle => "toggle".into(),
            Action::Next => "next".into(),
            Action::Previous => "previous".into(),
            Action::Seek { position_ms } => format!("seek {position_ms}"),
        }
    }
}

pub const MAX_SEEK_MS: u64 = 24 * 3600 * 1000;

/// Where an action goes: adapter arguments after the three paths, or one AppleScript for `app` ("Music" or
/// "Spotify", the process name `pgrep -x` checks).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Route {
    Adapter(Vec<String>),
    Script { app: &'static str, script: String },
}

const ADAPTER_SEND_TIMEOUT: Duration = Duration::from_secs(5);

pub fn parse_action(v: &Value) -> Result<Action, String> {
    let action: Action = serde_json::from_value(v.clone()).map_err(|e| e.to_string())?;
    // serde's internally tagged enums let unit variants ignore extra fields, so the check is made here.
    let fields = v.as_object().map_or(0, |o| o.len());
    let expected = if matches!(action, Action::Seek { .. }) { 2 } else { 1 };
    if fields != expected {
        return Err("unexpected field".into());
    }
    match action {
        Action::Seek { position_ms } if position_ms > MAX_SEEK_MS => Err("position_ms too large".into()),
        _ => Ok(action),
    }
}

/// A seek position past the track's known duration is clamped to it.
pub fn clamp(media: Option<&Media>, action: Action) -> Action {
    let duration = media.and_then(|m| m.track.as_ref()).and_then(|t| t.duration_ms);
    match (action, duration) {
        (Action::Seek { position_ms }, Some(d)) => Action::Seek { position_ms: position_ms.min(d) },
        _ => action,
    }
}

pub fn route(media: Option<&Media>, action: Action) -> Result<Route, String> {
    let Some(m) = media else { return Err("nothing playing".into()) };
    let Some(track) = &m.track else { return Err("nothing playing".into()) };
    let action = clamp(media, action);
    match m.via {
        Via::Adapter => Ok(Route::Adapter(match action {
            Action::Toggle => vec!["send".into(), "2".into()],
            Action::Next => vec!["send".into(), "4".into()],
            Action::Previous => vec!["send".into(), "5".into()],
            Action::Seek { position_ms } => vec!["seek".into(), (position_ms * 1000).to_string()],
        })),
        Via::Applescript => {
            let app = match track.bundle_id.as_str() {
                "com.apple.Music" => "Music",
                "com.spotify.client" => "Spotify",
                other => return Err(format!("cannot control {other}")),
            };
            let verb = match action {
                Action::Toggle => "playpause".to_string(),
                Action::Next => "next track".to_string(),
                Action::Previous => "previous track".to_string(),
                Action::Seek { position_ms } => {
                    format!("set player position to {}.{:03}", position_ms / 1000, position_ms % 1000)
                }
            };
            Ok(Route::Script { app, script: format!("tell application \"{app}\" to {verb}") })
        }
        Via::None => Err("no media source".into()),
    }
}

pub fn apply(route: &Route) -> Result<(), String> {
    match route {
        Route::Adapter(args) => {
            let args: Vec<&str> = args.iter().map(String::as_str).collect();
            let mut cmd = adapter_command(&adapter_dir(), &args);
            cmd.stdout(Stdio::null()).stderr(Stdio::null());
            let mut group = Group::spawn(cmd).map_err(|e| format!("adapter did not start: {e}"))?;
            match wait_deadline(&mut group.child, ADAPTER_SEND_TIMEOUT) {
                Ok(Some(status)) if status.success() => Ok(()),
                Ok(Some(status)) => Err(format!("adapter {status}")),
                Ok(None) => Err(format!("adapter timed out after {} s", ADAPTER_SEND_TIMEOUT.as_secs())),
                Err(e) => Err(format!("adapter wait: {e}")),
            }
        }
        Route::Script { app, script } => {
            if !running(app) {
                return Err(format!("{app} is not running"));
            }
            let out = osascript(script)?;
            if out.status.success() {
                return Ok(());
            }
            let stderr = String::from_utf8_lossy(&out.stderr);
            if applescript_denied(&stderr) {
                Err("denied".into())
            } else {
                Err(format!("osascript {}", out.status))
            }
        }
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
                sink(Media { via: Via::Adapter, health: Health::Failed, track: None });
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
            let report = Media { via: provider.via, health: Health::Failed, track: None };
            if let Err(panic) = std::panic::catch_unwind(AssertUnwindSafe(|| sink(report))) {
                eprintln!("media: Failed: reporting a panic: {}", panic_reason(panic.as_ref()));
            }
            provider.back_off();
        }
    }
}

#[cfg(test)]
#[path = "media_tests.rs"]
mod tests;
