//! Derives per-session status rows from the records the hook writes. See CONTRACT.md section 2.

use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
pub struct Record {
    pub session_id: String,
    pub cwd: String,
    pub event: String,
    #[serde(default)]
    pub notification_type: Option<String>,
    #[serde(default)]
    pub tool_name: Option<String>,
    pub ts: u64,
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

pub const TTL_SECS: u64 = 3 * 3600;
pub const MAX_ROWS: usize = 6;
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

pub fn summarize(records: &[Record], now: u64) -> Summary {
    let mut kept: Vec<(&Record, Status)> = records
        .iter()
        .filter(|r| now.saturating_sub(r.ts) <= TTL_SECS)
        .filter_map(|r| status_of(r).map(|s| (r, s)))
        .collect();

    let total = kept.len();
    let attention = kept
        .iter()
        .filter(|(_, s)| matches!(s, Status::NeedsPermission | Status::NeedsInput))
        .count();
    let working = kept.iter().filter(|(_, s)| *s == Status::Working).count();

    kept.sort_by(|(a, sa), (b, sb)| {
        sa.priority()
            .cmp(&sb.priority())
            .then(b.ts.cmp(&a.ts))
            .then(a.session_id.cmp(&b.session_id))
    });

    let labels: Vec<String> = kept.iter().map(|(r, _)| base_label(&r.cwd)).collect();
    let rows = kept
        .iter()
        .zip(&labels)
        .take(MAX_ROWS)
        .map(|((r, status), label)| {
            let shared = labels.iter().filter(|l| *l == label).count() > 1;
            let label = if shared {
                let short: String = r.session_id.chars().take(4).collect();
                format!("{label} · {short}")
            } else {
                label.clone()
            };
            let detail = match status {
                Status::NeedsPermission => r.tool_name.clone(),
                _ => None,
            };
            Row { label, status: *status, detail }
        })
        .collect();

    Summary { rows, total, attention, working }
}

pub fn read_dir(dir: &Path) -> Vec<Record> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut paths: Vec<PathBuf> = entries
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|ext| ext == "json"))
        .collect();
    paths.sort();
    paths.truncate(MAX_FILES);
    paths
        .iter()
        .filter_map(|p| serde_json::from_slice::<Record>(&std::fs::read(p).ok()?).ok())
        .collect()
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
