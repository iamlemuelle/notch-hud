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

Known limit: a manual "No" or Esc on a permission prompt sends no event. The session stays `NeedsPermission` until
the main thread's next boundary event (section 2), normally `Notification` `idle_prompt` or your next prompt.
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
2. Group kept records by `session_id`. Superseding: when a group has a main-thread record (`agent_id == None`) that
   is a turn boundary (`UserPromptSubmit`, `Stop`, `StopFailure`, `SessionStart`, `PermissionDenied`, or `Notification`
   with `idle_prompt`), drop that group's agent records whose `ts` is strictly older than the main record's `ts`. An
   interrupted subagent sends no `SubagentStop`, and a manual "No" or Esc on a permission prompt sends no
   `PermissionDenied`; the main thread's next boundary event is what clears them. Each group is then one session:
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

- Startup: when no screen in `NSScreen::screens` has `safeAreaInsets().top > 0`, print `notch-hud: no notched display`
  to stderr and exit 0. Set window level, collection behavior and shadow once.
- Every time the window is placed (startup, hover change, row-count change), look the notched screen up again on the
  main thread, so plugging in or rearranging displays moves the panel correctly. If none is found, leave the frame.
- Every 1 s: `prune(&dir, now)`, then `summarize(&read_dir(&dir), now)` with `dir = sessions_dir()`. Also every 1 s, on
  the main thread: compute the frame the panel should have now (notched screen looked up again, current expanded state)
  and set it when it differs from the window's frame, so a display change moves the collapsed pill too. When it differs from the last one, emit `sessions` with the
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

## 6. Login item

`scripts/install-login.sh [plist]` (default `~/Library/LaunchAgents/com.lem.notch-hud.plist`) starts the app at login:

- The binary is `$NOTCH_HUD_BIN` if set, else this repo's `src-tauri/target/release/notch-hud`, as an absolute path.
  When it is not an executable file, print `notch-hud: build first: cd src-tauri && cargo build --release` to stderr,
  exit 1, write nothing, and call `launchctl` not at all.
- A binary path containing `&`, `<` or `>` would break the plist's XML: print a message to stderr and exit 1, writing
  nothing.
- Writes the plist through a temp file in the same directory that `plutil -lint` must accept, then `mv`, mode 0644,
  creating the directory if missing. Keys: `Label` `com.lem.notch-hud`; `ProgramArguments` `[<binary>]`; `RunAtLoad`
  true; `KeepAlive` `{SuccessfulExit: false}` (restart after a crash, not after a clean exit such as "no notched
  display"); `ProcessType` `Interactive`; `StandardErrorPath` `$HOME/Library/Logs/notch-hud.log`.
- Then runs `launchctl bootout gui/<uid>/com.lem.notch-hud` (its failure, meaning "not loaded", is ignored), with
  `<uid>` from `id -u`. `bootout` returns before the old service is gone, and a `bootstrap` while it is still there
  fails with exit 5 (seen on macOS 27.0.1). So the script polls `launchctl print gui/<uid>/com.lem.notch-hud` every
  0.1 s until it exits non-zero, for at most 5 s. If the service is still there after 5 s, it prints
  `notch-hud: old login item did not stop` to stderr and exits 1 without calling `bootstrap`. Otherwise it runs
  `launchctl bootstrap gui/<uid> <plist>`. A failing bootstrap makes the script exit non-zero. `launchctl` is found
  on `PATH`.
- On success prints one line, `notch-hud: login item installed: <plist>`. Running it again rewrites and reloads.

`scripts/uninstall-login.sh [plist]` runs `launchctl bootout gui/<uid>/com.lem.notch-hud` (failure ignored), deletes the
plist if present, and exits 0, also when nothing was installed.

## 7. Now Playing (`src-tauri/src/media.rs`)

Read-only in this version: the panel shows what is playing. Play, pause, next and seek come with click routing.

### Sources, in order

1. **Adapter** (primary). The vendored `mediaremote-adapter` (see `vendor/VENDORED.md`), built by
   `scripts/build-adapter.sh` into a directory holding `mediaremote-adapter.pl`, `MediaRemoteAdapter.framework` and
   `MediaRemoteAdapterTestClient`. That directory is `$NOTCH_MEDIA_ADAPTER` if set and non-empty, else `../adapter`
   next to the running binary (`src-tauri/target/adapter` for both build profiles). It reports any app macOS lists
   as Now Playing, browsers included, so YouTube in Chrome or Safari shows up as that browser.
   - Usable when `/usr/bin/perl <dir>/mediaremote-adapter.pl <dir>/MediaRemoteAdapter.framework
     <dir>/MediaRemoteAdapterTestClient test` exits 0 within 5 s.
   - Then runs `... stream --micros --no-diff --debounce=100` and reads one JSON object per stdout line.
2. **AppleScript** (fallback, when the adapter is not usable). Every 2 s, for each of Music (`pgrep -x Music`) and
   Spotify (`pgrep -x Spotify`) that is running, one `osascript` call prints one tab-separated line per app:
   `<music|spotify>\t<playing|paused|stopped>\t<title>\t<artist>\t<album>\t<duration s>\t<position s>\t<artwork url or empty>`.
   Stderr containing `-1743` means Automation access was refused: health `denied`, no retry until the next poll.

When the adapter stream exits, the provider waits `backoff(attempt)` (1 s doubling, capped at 60 s, attempt resets
after a stream that lived 60 s or more), runs `test` again and picks a source again. A panic inside the provider is
caught; it logs `media: Failed: <reason>` to stderr and restarts under the same backoff. When the adapter is not
usable it logs `media: adapter Failed (<reason>), using AppleScript` once per switch.

### Model

```rust
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Track {
    pub title: String,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub app: String,                 // app_name(...)
    pub bundle_id: String,
    pub playing: bool,
    pub duration_ms: Option<u64>,
    pub elapsed_ms: Option<u64>,     // position at elapsed_at_ms
    pub elapsed_at_ms: Option<u64>,  // Unix epoch milliseconds
    pub artwork: Option<String>,     // "data:<mime>;base64,<data>" or an https URL
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Via { Adapter, Applescript, None }

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Health { Ok, Denied, Failed }

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Media { pub via: Via, pub health: Health, pub track: Option<Track> }
```

### Pure functions

- `parse_adapter_line(line: &str) -> Result<Option<Track>, String>`. Input is one stream line,
  `{"type":"data","diff":false,"payload":{...}}`. `Err` for invalid JSON, a `type` other than `"data"`, or
  `diff: true`. `Ok(None)` for an empty payload or one without a non-empty `title`. Otherwise `Ok(Some)` with:
  `bundle_id` = `bundleIdentifier`; `app` = `app_name(bundleIdentifier, parentApplicationBundleIdentifier)`;
  `playing` = `playing` (false if absent); `duration_ms` = `durationMicros / 1000`; `elapsed_ms` =
  `elapsedTimeMicros / 1000`; `elapsed_at_ms` = `timestampEpochMicros / 1000` (each `None` if absent);
  `artwork` = `data:<artworkMimeType>;base64,<artworkData>` only when both are present and non-empty.
  Real recorded lines are in `tests/media/fixtures/adapter-stream.jsonl`.
- `app_name(bundle_id: &str, parent: Option<&str>) -> String`. Looks up `parent` when given, else `bundle_id`:
  `com.apple.Music` Apple Music, `com.spotify.client` Spotify, `com.google.Chrome` Chrome, `com.apple.Safari`
  Safari, `company.thebrowser.Browser` Arc, `org.mozilla.firefox` Firefox, `com.microsoft.edgemac` Edge,
  `com.brave.Browser` Brave, `com.apple.podcasts` Podcasts, `com.apple.QuickTimePlayerX` QuickTime Player.
  Anything else returns the looked-up identifier unchanged.
- `parse_applescript(out: &str, now_ms: u64) -> Option<Track>`. Reads the lines above. Picks the first `playing`
  line, else the first `paused` line; `stopped`, malformed lines (fewer than 8 fields, unknown app, unparsable
  numbers) and empty titles are skipped. `music` maps to `com.apple.Music` / Apple Music, `spotify` to
  `com.spotify.client` / Spotify. Seconds become milliseconds (rounded); `elapsed_at_ms` = `now_ms`; empty artist,
  album or artwork become `None`.
- `applescript_denied(stderr: &str) -> bool`: true when `stderr` contains `-1743`.
- `backoff(attempt: u32) -> std::time::Duration`: `min(2^attempt, 60)` seconds.

### App and UI

- The app keeps the latest `Media` and emits `media` with it whenever it changes; the `media` command returns it
  (`null` before the first update).
- Expanded height adds 108 pt when `track` is set, or 22 pt when `track` is null and `health` is `denied`:
  `44 + extra + 22 * max(rows.len(), 1) + 14`.
- Collapsed right ear: `⚠ <attention>` when `attention > 0` (unchanged), else, when a track is set, its artwork
  (18 pt square, or 26 × 15 pt when the artwork is at least 1.3 times wider than tall) and four equalizer bars while
  playing or a pause glyph while paused.
- Expanded: below the ear band, the artwork (64 pt square, or 112 × 63 pt when wide), title, artist and app name,
  then a progress bar with elapsed and remaining time (`h:mm:ss` from one hour) that the page advances while
  playing: `elapsed_ms + (now - elapsed_at_ms)`, clamped to `duration_ms`. Session rows follow. With `denied`, one
  muted line: `Music access is off: allow notch-hud in System Settings, Privacy & Security, Automation`.
