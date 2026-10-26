//! Sends the prompt box's text to a Claude Code session's inbox socket. See CONTRACT.md section 11, "Sending".

use std::io::Write;
use std::os::unix::fs::{FileTypeExt, MetadataExt};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::Duration;

use crate::session::{valid_id, Record};

/// How the detail card can reach a session.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(tag = "kind", content = "host", rename_all = "snake_case")]
pub enum Reach {
    Inbox,
    /// No usable inbox; the bundle id of the app the session runs in.
    HostOnly(String),
    None,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Prompt {
    pub id: String,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Route {
    Inbox(PathBuf),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SendError {
    /// The path is not a socket of ours: nothing was attempted.
    Rejected(String),
    /// Connecting or writing failed or timed out.
    Failed(String),
}

pub const MAX_CHARS: usize = 8000;
/// `sockaddr_un.sun_path` on macOS: 103 bytes of path and a NUL.
pub const SUN_PATH_MAX: usize = 104;
pub const TIMEOUT: Duration = Duration::from_secs(2);

pub fn parse_prompt(id: &str, text: &str) -> Result<Prompt, String> {
    if !valid_id(id) {
        return Err("bad session id".into());
    }
    let text = text.trim();
    let n = text.chars().count();
    if n == 0 {
        return Err("empty text".into());
    }
    if n > MAX_CHARS {
        return Err(format!("text is {n} chars, over {MAX_CHARS}"));
    }
    if text.contains('\0') {
        return Err("text contains NUL".into());
    }
    Ok(Prompt { id: id.to_string(), text: text.to_string() })
}

fn main_record<'a>(records: &'a [Record], id: &str) -> Option<&'a Record> {
    records.iter().filter(|r| r.session_id == id && r.agent_id.is_none()).max_by_key(|r| r.ts)
}

pub fn route(records: &[Record], id: &str) -> Result<Route, String> {
    let main = main_record(records, id).ok_or("no record for this session")?;
    let inbox = main.inbox.as_deref().filter(|p| !p.is_empty()).ok_or("no inbox in the record")?;
    let path = Path::new(inbox);
    if !path.is_absolute() {
        return Err("inbox path is not absolute".into());
    }
    if inbox.len() >= SUN_PATH_MAX {
        return Err(format!("inbox path is {} bytes, the limit is {}", inbox.len(), SUN_PATH_MAX - 1));
    }
    // Socket names are `<pid>.sock` and outlive their session, so after pid reuse two sessions can name one socket.
    let naming = records.iter().filter(|r| r.inbox.as_deref() == Some(inbox));
    let (ours, others): (Vec<&Record>, Vec<&Record>) = naming.partition(|r| r.session_id == id);
    let ours = ours.iter().map(|r| r.ts).fold(main.ts, u64::max);
    if others.iter().any(|r| r.ts >= ours) {
        return Err("inbox is shared with a session as new or newer".into());
    }
    Ok(Route::Inbox(path.to_path_buf()))
}

pub fn reach(records: &[Record], id: &str) -> Reach {
    if route(records, id).is_ok() {
        return Reach::Inbox;
    }
    match main_record(records, id).and_then(|r| r.host.as_deref()).filter(|h| !h.is_empty()) {
        Some(host) => Reach::HostOnly(host.to_string()),
        None => Reach::None,
    }
}

/// Written by hand so the keys keep this order: a `serde_json` map would sort them.
pub fn frame(text: &str) -> String {
    let content = serde_json::to_string(text).expect("a str always serializes");
    format!("{{\"type\":\"user\",\"message\":{{\"role\":\"user\",\"content\":{content}}}}}\n")
}

extern "C" {
    fn getuid() -> u32;
}

pub fn send(path: &Path, line: &str) -> Result<(), SendError> {
    let meta = std::fs::symlink_metadata(path).map_err(|e| SendError::Rejected(format!("lstat: {e}")))?;
    let kind = meta.file_type();
    if kind.is_symlink() {
        return Err(SendError::Rejected("inbox is a symlink".into()));
    }
    if !kind.is_socket() {
        return Err(SendError::Rejected("inbox is not a socket".into()));
    }
    // SAFETY: getuid has no preconditions and cannot fail.
    if meta.uid() != unsafe { getuid() } {
        return Err(SendError::Rejected("inbox belongs to another user".into()));
    }
    let mut stream = connect(path)?;
    stream.set_write_timeout(Some(TIMEOUT)).map_err(|e| SendError::Failed(format!("write timeout: {e}")))?;
    stream.write_all(line.as_bytes()).map_err(|e| SendError::Failed(format!("write: {e}")))
}

/// std has no connect timeout for Unix sockets, so the connect runs on its own thread. One that finishes after the
/// timeout finds the receiver gone and drops the stream unwritten.
fn connect(path: &Path) -> Result<UnixStream, SendError> {
    let (tx, rx) = mpsc::channel();
    let target = path.to_path_buf();
    std::thread::spawn(move || {
        let _ = tx.send(UnixStream::connect(target));
    });
    match rx.recv_timeout(TIMEOUT) {
        Ok(Ok(stream)) => Ok(stream),
        Ok(Err(e)) => Err(SendError::Failed(format!("connect: {e}"))),
        Err(_) => Err(SendError::Failed("connect timed out".into())),
    }
}

#[cfg(test)]
#[path = "prompt_tests.rs"]
mod tests;
