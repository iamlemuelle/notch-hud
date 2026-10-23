//! Derives per-session status rows from the records the hook writes. See CONTRACT.md section 2.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
pub struct Record {
    pub session_id: String,
    pub cwd: String,
    pub event: String,
    #[serde(default)]
    pub agent_id: Option<String>,
    #[serde(default)]
    pub notification_type: Option<String>,
    #[serde(default)]
    pub tool_name: Option<String>,
    pub ts: u64,
    #[serde(default)]
    pub inbox: Option<String>,
    #[serde(default)]
    pub host: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    NeedsPermission,
    NeedsInput,
    Error,
    Working,
    Done,
    Waiting,
}

impl Status {
    fn priority(self) -> u8 {
        match self {
            Status::NeedsPermission => 0,
            Status::NeedsInput => 1,
            Status::Error => 2,
            Status::Working => 3,
            Status::Done => 4,
            Status::Waiting => 5,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Row {
    pub id: String,
    pub label: String,
    pub status: Status,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Summary {
    pub rows: Vec<Row>,
    pub total: usize,
    pub attention: usize,
    pub working: usize,
}

/// One session as the detail card shows it. See CONTRACT.md section 2, "Session ids and `detail`".
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Detail {
    pub id: String,
    pub label: String,
    pub cwd: String,
    pub status: Status,
    pub event: String,
    pub event_ts: u64,
    pub tool_name: Option<String>,
    pub agents: usize,
}

pub const TTL_SECS: u64 = 3 * 3600;
pub const QUIET_SECS: u64 = 600;
pub const MAX_ROWS: usize = 4;
pub const MAX_FILES: usize = 50;

pub fn status_of(r: &Record) -> Option<Status> {
    match (r.event.as_str(), r.notification_type.as_deref()) {
        ("SessionStart", _) => Some(Status::Waiting),
        ("UserPromptSubmit" | "PreToolUse" | "PostToolUse" | "PostToolUseFailure", _) => {
            Some(Status::Working)
        }
        ("PermissionRequest", _) => Some(Status::NeedsPermission),
        ("Notification", Some("permission_prompt")) => Some(Status::NeedsPermission),
        ("Notification", Some("idle_prompt")) => Some(Status::Waiting),
        ("Notification", Some("elicitation_dialog" | "agent_needs_input")) => {
            Some(Status::NeedsInput)
        }
        ("PermissionDenied", _) => Some(Status::Waiting),
        ("Stop", _) => Some(Status::Done),
        ("StopFailure", _) => Some(Status::Error),
        _ => None,
    }
}

fn base_label(cwd: &str) -> String {
    cwd.rsplit('/')
        .find(|part| !part.is_empty())
        .unwrap_or(cwd)
        .to_string()
}

struct Session {
    id: String,
    cwd: String,
    status: Status,
    detail: Option<String>,
    ts: u64,
    /// The newest record left in the group (its `ts` is `ts`): its event and tool.
    event: String,
    tool_name: Option<String>,
    agents: usize,
}

fn is_turn_boundary(r: &Record) -> bool {
    r.agent_id.is_none()
        && match r.event.as_str() {
            "UserPromptSubmit" | "Stop" | "StopFailure" | "SessionStart" | "PermissionDenied" => true,
            "Notification" => r.notification_type.as_deref() == Some("idle_prompt"),
            _ => false,
        }
}

/// A main-thread turn boundary clears agent records older than it: an interrupted subagent sends no `SubagentStop`.
fn supersede_agents(group: &mut Vec<(&Record, Status)>) {
    let Some(boundary_ts) = group.iter().filter(|(r, _)| is_turn_boundary(r)).map(|(r, _)| r.ts).max() else {
        return;
    };
    group.retain(|(r, _)| r.agent_id.is_none() || r.ts >= boundary_ts);
}

fn collapse(group: &[(&Record, Status)], now: u64) -> Session {
    let (top, top_status) = group
        .iter()
        .min_by_key(|(_, s)| s.priority())
        .expect("group is non-empty");
    let ts = group.iter().map(|(r, _)| r.ts).max().unwrap_or(0);
    let status = if *top_status == Status::Working && now.saturating_sub(ts) > QUIET_SECS {
        Status::Waiting
    } else {
        *top_status
    };
    let cwd = group
        .iter()
        .find(|(r, _)| r.agent_id.is_none())
        .map_or(&top.cwd, |(r, _)| &r.cwd)
        .clone();
    let detail = match status {
        Status::NeedsPermission => top.tool_name.clone(),
        _ => None,
    };
    // Ties go to the main thread, then to the smallest agent id.
    let (newest, _) = group
        .iter()
        .max_by(|(a, _), (b, _)| {
            a.ts.cmp(&b.ts)
                .then(a.agent_id.is_none().cmp(&b.agent_id.is_none()))
                .then(b.agent_id.cmp(&a.agent_id))
        })
        .expect("group is non-empty");
    Session {
        id: top.session_id.clone(),
        cwd,
        status,
        detail,
        ts,
        event: newest.event.clone(),
        tool_name: newest.tool_name.clone(),
        agents: group.iter().filter(|(r, _)| r.agent_id.is_some()).count(),
    }
}

/// Records within `TTL_SECS` of `now` that have a status.
fn kept(records: &[Record], now: u64) -> impl Iterator<Item = (&Record, Status)> {
    records
        .iter()
        .filter(move |r| now.saturating_sub(r.ts) <= TTL_SECS)
        .filter_map(|r| Some((r, status_of(r)?)))
}

fn sessions(records: &[Record], now: u64) -> Vec<Session> {
    let mut groups: BTreeMap<&str, Vec<(&Record, Status)>> = BTreeMap::new();
    for (r, s) in kept(records, now) {
        groups.entry(r.session_id.as_str()).or_default().push((r, s));
    }
    groups
        .values_mut()
        .map(|g| {
            supersede_agents(g);
            collapse(g, now)
        })
        .collect()
}

pub fn summarize(records: &[Record], now: u64) -> Summary {
    let mut sessions = sessions(records, now);

    let total = sessions.len();
    let attention = sessions
        .iter()
        .filter(|s| matches!(s.status, Status::NeedsPermission | Status::NeedsInput | Status::Error))
        .count();
    let working = sessions.iter().filter(|s| s.status == Status::Working).count();

    sessions.sort_by(|a, b| {
        a.status
            .priority()
            .cmp(&b.status.priority())
            .then(b.ts.cmp(&a.ts))
            .then(a.id.cmp(&b.id))
    });

    let rows = sessions
        .iter()
        .zip(labels(&sessions))
        .take(MAX_ROWS)
        .map(|(s, label)| Row { id: s.id.clone(), label, status: s.status, detail: s.detail.clone() })
        .collect();

    Summary { rows, total, attention, working }
}

/// Each session's label: the last folder name, with a short id added when two sessions share it.
fn labels(sessions: &[Session]) -> Vec<String> {
    let bases: Vec<String> = sessions.iter().map(|s| base_label(&s.cwd)).collect();
    sessions
        .iter()
        .zip(&bases)
        .map(|(s, base)| {
            if bases.iter().filter(|b| *b == base).count() > 1 {
                let short: String = s.id.chars().take(4).collect();
                format!("{base} · {short}")
            } else {
                base.clone()
            }
        })
        .collect()
}

/// The hook's rule for ids: non-empty and only `[A-Za-z0-9_-]`.
pub fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

pub fn detail(records: &[Record], id: &str, now: u64) -> Option<Detail> {
    let sessions = sessions(records, now);
    let labels = labels(&sessions);
    let (s, label) = sessions.into_iter().zip(labels).find(|(s, _)| s.id == id)?;
    Some(Detail {
        id: s.id,
        label,
        cwd: s.cwd,
        status: s.status,
        event: s.event,
        event_ts: s.ts,
        tool_name: s.tool_name,
        agents: s.agents,
    })
}

/// The next second a record expires or a `Working` session decays, so the app can sleep until then.
pub fn next_wake(records: &[Record], now: u64) -> Option<u64> {
    let expiries = kept(records, now).map(|(r, _)| r.ts.saturating_add(TTL_SECS + 1));
    let decays = sessions(records, now)
        .into_iter()
        .filter(|s| s.status == Status::Working)
        .map(|s| s.ts.saturating_add(QUIET_SECS + 1));
    expiries.chain(decays).min()
}

/// Regular `*.json` files in `dir` with their mtimes, unsorted.
fn json_files(dir: &Path) -> Vec<(PathBuf, SystemTime)> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    entries
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|ext| ext == "json"))
        .filter_map(|e| {
            let meta = e.metadata().ok()?;
            meta.is_file().then(|| Some((e.path(), meta.modified().ok()?)))?
        })
        .collect()
}

pub fn read_dir(dir: &Path) -> Vec<Record> {
    let mut files = json_files(dir);
    files.sort_by(|(pa, ma), (pb, mb)| mb.cmp(ma).then_with(|| pa.file_name().cmp(&pb.file_name())));
    files.truncate(MAX_FILES);
    files
        .iter()
        .filter_map(|(p, _)| serde_json::from_slice::<Record>(&std::fs::read(p).ok()?).ok())
        .collect()
}

pub fn prune(dir: &Path, now: u64) {
    let cutoff = UNIX_EPOCH + Duration::from_secs(now.saturating_sub(TTL_SECS));
    for (path, mtime) in json_files(dir) {
        if mtime < cutoff {
            let _ = std::fs::remove_file(path);
        }
    }
}

pub fn sessions_dir() -> PathBuf {
    match std::env::var("NOTCH_DIR") {
        Ok(dir) if !dir.is_empty() => PathBuf::from(dir),
        _ => PathBuf::from(std::env::var("HOME").unwrap_or_default())
            .join(".claude/notch/sessions"),
    }
}

#[cfg(test)]
#[path = "session_tests.rs"]
mod tests;
