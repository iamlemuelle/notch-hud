# Live session status: contract

Claude Code hooks record each session's latest event in a file. The notch app reads those files, derives a status per
session, and renders them. Code and tests are written against this file, by different agents.

## 1. Session record (written by `hooks/notch-hook.sh`)

Directory: `$NOTCH_DIR` if set, else `~/.claude/notch/sessions`. One file per session: `<session_id>.json`.

```json
{"session_id":"abc123","cwd":"/Users/lem/bianki","event":"PermissionRequest","notification_type":null,"tool_name":"Bash","ts":1792560000}
```

- `event` is the hook's `hook_event_name`. `notification_type` and `tool_name` are copied from the hook input, or `null`.
- `ts` is integer unix seconds at the time the hook ran.

The hook reads one JSON object on stdin and:

1. Exits 0 without writing when `session_id` is missing or does not match `^[A-Za-z0-9_-]+$` (no path traversal).
2. On `SessionEnd`: deletes `<session_id>.json` if present.
3. When `agent_id` is present (the event came from a subagent): writes only for `PermissionRequest` and `Notification`;
   every other subagent event is skipped, so a busy subagent never hides a permission prompt.
4. On `Notification`: writes only when `notification_type` is one of `permission_prompt`, `idle_prompt`,
   `elicitation_dialog`, `agent_needs_input`. Other types are skipped.
5. Otherwise writes the record atomically: a temp file in the same directory, then `mv`. Creates the directory if missing.

Always: prints nothing on stdout or stderr, and exits 0, including on malformed JSON, an unwritable directory, or a
missing `jq`. (Exit 2 on `PreToolUse` would block the user's tool call; stdout on `SessionStart`/`UserPromptSubmit`
would be injected into Claude's context.)

Registered synchronously (not `async`) with `"timeout": 2` for: `SessionStart`, `UserPromptSubmit`, `PreToolUse`,
`PostToolUse`, `PostToolUseFailure`, `PermissionRequest`, `Notification`, `Stop`, `StopFailure`, `SessionEnd`.
Async hooks can finish out of order, which could let a late `PostToolUse` overwrite `Stop`.

## 2. Rust model (`src-tauri/src/session.rs`)

```rust
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
pub enum Status { NeedsPermission, NeedsInput, Error, Working, Done, Waiting }

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Row { pub label: String, pub status: Status, pub detail: Option<String> }

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Summary { pub rows: Vec<Row>, pub total: usize, pub attention: usize, pub working: usize }

pub const TTL_SECS: u64 = 3 * 3600;
pub const MAX_ROWS: usize = 6;
pub const MAX_FILES: usize = 50;

pub fn status_of(r: &Record) -> Option<Status>;
pub fn summarize(records: &[Record], now: u64) -> Summary;
pub fn read_dir(dir: &std::path::Path) -> Vec<Record>;
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
| `Stop` | | `Done` |
| `StopFailure` | | `Error` |
| anything else | | `None` (record ignored) |

### `summarize(records, now)`

1. Keep records where `status_of` is `Some` and age `now.saturating_sub(ts) <= TTL_SECS` (a future `ts` has age 0).
2. `total` = kept count. `attention` = kept `NeedsPermission` + `NeedsInput`. `working` = kept `Working`.
3. `label` = last non-empty `/`-separated component of `cwd`; `cwd` itself if there is none. When two or more kept
   records share a label, each of them becomes `"<label> · <first 4 chars of session_id>"`.
4. `detail` = `tool_name` when the status is `NeedsPermission`, else `None`.
5. Sort by status priority (`NeedsPermission` 0, `NeedsInput` 1, `Error` 2, `Working` 3, `Done` 4, `Waiting` 5),
   then `ts` descending, then `session_id` ascending. Then truncate `rows` to `MAX_ROWS`.

### `read_dir(dir)`

Reads at most `MAX_FILES` `*.json` files (sorted by file name), parses each as a `Record`, skips any file that fails
to read or parse. A missing directory returns an empty list. Never panics.

### `sessions_dir()`

`$NOTCH_DIR` if set and non-empty, else `$HOME/.claude/notch/sessions`.

## 3. App behavior

- Startup: pick the screen from `NSScreen::screens` whose `safeAreaInsets().top > 0`. Store its geometry once. Without
  one, print `notch-spike: no notched display` to stderr and exit 0. Set window level, collection behavior and shadow
  once.
- Every 1 s: `summarize(&read_dir(&sessions_dir()), now)`. When it differs from the last one, emit `sessions` with the
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

`scripts/install-hooks.sh [settings.json]` (default `~/.claude/settings.json`): backs the file up as
`<file>.notch-backup-<unix ts>`, removes any hook group whose command is this repo's `hooks/notch-hook.sh`, then
appends one group per event in section 1. Running it twice leaves one registration per event. All other hooks are kept.
`scripts/uninstall-hooks.sh [settings.json]` removes exactly those groups and drops an event key left empty.
