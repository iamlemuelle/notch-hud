# Live session status: contract

Claude Code hooks record each session's latest event in a file. The notch app reads those files, derives a status per
session, and renders them. Code and tests are written against this file, by different agents.

## 1. Session records (written by `hooks/notch-hook.sh`)

Directory: `$NOTCH_DIR` if set, else `~/.claude/notch/sessions`. One file per session and agent:
`<session_id>.json` for the main thread, `<session_id>.<agent_id>.json` for a subagent (input has `agent_id`).

```json
{"session_id":"abc123","agent_id":null,"cwd":"/Users/lem/bianki","event":"PermissionRequest","notification_type":null,"tool_name":"Bash","ts":1792560000}
```

- `event` is the hook's `hook_event_name`. `agent_id`, `notification_type` and `tool_name` are copied from the hook
  input, or `null`. `ts` is integer unix seconds at the time the hook ran.

The hook reads one JSON object on stdin and:

1. Exits 0 without writing when `session_id` is missing or is not entirely `[A-Za-z0-9_-]+` (anchored at both ends of
   the string; a trailing newline is invalid). The same rule applies to `agent_id` when present.
2. On `SessionEnd`: deletes `<session_id>.json` and every `<session_id>.*.json`.
3. On `SubagentStop`: deletes `<session_id>.<agent_id>.json`.
4. On `Notification`: writes only when `notification_type` is one of `permission_prompt`, `idle_prompt`,
   `elicitation_dialog`, `agent_needs_input`. Other types are skipped.
5. Otherwise writes the record for that session and agent atomically: a temp file in the same directory whose name does
   not end in `.json`, then `mv`. Creates the directory if missing.

Always: prints nothing on stdout or stderr, and exits 0, including on malformed JSON, an unwritable directory, or a
missing `jq`. (Exit 2 on `PreToolUse` would block the user's tool call; stdout on `SessionStart`/`UserPromptSubmit`
would be injected into Claude's context.)

Registered synchronously (not `async`) with `"timeout": 2` for: `SessionStart`, `UserPromptSubmit`, `PreToolUse`,
`PostToolUse`, `PostToolUseFailure`, `PermissionRequest`, `PermissionDenied`, `Notification`, `Stop`, `StopFailure`,
`SubagentStop`, `SessionEnd`. Async hooks can finish out of order, which could let a late `PostToolUse` overwrite `Stop`.

Known limit: Claude Code has no event for "permission approved", so after you approve a long-running tool the session
shows `NeedsPermission` until that tool's `PostToolUse`.

## 2. Rust model (`src-tauri/src/session.rs`)

```rust
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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Status { NeedsPermission, NeedsInput, Error, Working, Done, Waiting }

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Row { pub label: String, pub status: Status, pub detail: Option<String> }

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Summary { pub rows: Vec<Row>, pub total: usize, pub attention: usize, pub working: usize }

pub const TTL_SECS: u64 = 3 * 3600;
pub const QUIET_SECS: u64 = 10 * 60;
pub const MAX_ROWS: usize = 6;
pub const MAX_FILES: usize = 50;

pub fn status_of(r: &Record) -> Option<Status>;
pub fn summarize(records: &[Record], now: u64) -> Summary;
pub fn read_dir(dir: &std::path::Path) -> Vec<Record>;
pub fn prune(dir: &std::path::Path, now: u64);
pub fn sessions_dir() -> std::path::PathBuf;
```

### `status_of`

| event | notification_type | status |
|---|---|---|
| `SessionStart` | | `Waiting` |
| `UserPromptSubmit`, `PreToolUse`, `PostToolUse`, `PostToolUseFailure` | | `Working` |
| `PermissionRequest` | | `NeedsPermission` |
| `Notification` | `permission_prompt` | `NeedsPermission` |
| `Notification` | `idle_prompt` | `Waiting` |
| `Notification` | `elicitation_dialog`, `agent_needs_input` | `NeedsInput` |
| `PermissionDenied` | | `Waiting` |
| `Stop` | | `Done` |
| `StopFailure` | | `Error` |
| anything else | | `None` (record ignored) |

### `summarize(records, now)`

1. Keep records where `status_of` is `Some` and age `now.saturating_sub(ts) <= TTL_SECS` (a future `ts` has age 0).
2. Group kept records by `session_id`. Each group is one session:
   - status: the highest-priority status among its records (priority below), taken from that record;
   - `ts`: the largest `ts` in the group;
   - quiet decay: when the chosen status is `Working` and `now.saturating_sub(ts) > QUIET_SECS`, the status is `Waiting`
     (Claude Code sends no `Stop` when you interrupt with Esc);
   - `detail`: that record's `tool_name` when the status is `NeedsPermission`, else `None`;
   - `cwd`: from the record with `agent_id == None` if there is one, else from the record the status came from.
3. `total` = session count. `attention` = sessions that are `NeedsPermission` or `NeedsInput`. `working` = sessions that
   are `Working` (after decay).
4. `label` = last non-empty `/`-separated component of the session's `cwd`; `cwd` itself if there is none. When two or
   more sessions share a label, each of them becomes `"<label> · <first 4 chars of session_id>"`.
5. Sort sessions by status priority (`NeedsPermission` 0, `NeedsInput` 1, `Error` 2, `Working` 3, `Done` 4, `Waiting` 5),
   then `ts` descending, then `session_id` ascending. Then truncate `rows` to `MAX_ROWS`.

### `read_dir(dir)`

Considers only regular files whose names end in `.json`. Keeps the `MAX_FILES` most recently modified (mtime
descending, file name ascending on ties), parses each as a `Record`, and skips any file that fails to read or parse.
A missing directory returns an empty list. Never panics.

### `prune(dir, now)`

Best effort, never panics: deletes every `*.json` file in `dir` whose mtime is more than `TTL_SECS` before `now`.
The app calls it before each `read_dir`.

### `sessions_dir()`

`$NOTCH_DIR` if set and non-empty, else `$HOME/.claude/notch/sessions`.

## 3. App behavior

- Startup: when no screen in `NSScreen::screens` has `safeAreaInsets().top > 0`, print `notch-spike: no notched display`
  to stderr and exit 0. Set window level, collection behavior and shadow once.
- Every time the window is placed (startup, hover change, row-count change), look the notched screen up again on the
  main thread, so plugging in or rearranging displays moves the panel correctly. If none is found, leave the frame.
- Every 1 s: `prune(&dir, now)`, then `summarize(&read_dir(&dir), now)` with `dir = sessions_dir()`. When it differs from the last one, emit `sessions` with the
  `Summary` as payload, and resize the panel if it is expanded.
- Hover (unchanged): native cursor polling every 50 ms, emits `hover` (bool).
- Expanded size: width 420, height `44 + 22 * max(rows.len(), 1) + 14` pt, top edge at the screen top, centered on the notch.

## 4. UI (`dist/index.html`)

- Collapsed left ear: a dot and `total`. Dot color: amber `#f5a623` when `attention > 0`; else green `#34d058` when
  `working > 0`; else gray `#8b8b8b`.
- Collapsed right ear: `⚠ <attention>` in amber when `attention > 0`, else empty.
- Expanded: one line per row, `<label> — <status text>`. Status text: `needs permission: <detail>` (or
  `needs permission`), `needs your input`, `error`, `working`, `done`, `waiting for you`. No rows: `no Claude Code sessions`.

## 5. Install

`scripts/install-hooks.sh [settings.json]` (default `~/.claude/settings.json`):
- follows a symlink to the real file and writes that file, so the link survives;
- backs it up first as a new, uniquely named `<file>.notch-backup-<unix ts>.<random>` (never overwrites a backup);
- removes any hook group containing this repo's `hooks/notch-hook.sh`, then appends one group per event in section 1,
  with the command shell-quoted;
- keeps the file's permission bits, and leaves no temp file behind when it fails (the settings file is then unchanged).

Running it twice leaves one registration per event. All other hooks are kept.
`scripts/uninstall-hooks.sh [settings.json]` removes exactly those groups, drops an event key that our removal left
empty, and follows the same symlink, backup, permission and temp-file rules.
