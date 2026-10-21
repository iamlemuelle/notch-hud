//! Now Playing: what the panel shows is playing. See CONTRACT.md section 7.

use std::time::Duration;

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

#[cfg(test)]
#[path = "media_tests.rs"]
mod tests;
