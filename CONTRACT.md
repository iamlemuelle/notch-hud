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
- Hover: native cursor polling every 50 ms, emits `hover` (bool). A left-button press that the poll first sees
  (`NSEvent::pressedMouseButtons() & 1` going from up to down) while the cursor is over the expanded panel is latched
  until the button is up again; while it is latched the panel stays expanded even when the cursor leaves it, so a seek
  drag that overshoots the bar does not collapse the panel under the pointer. A press that starts anywhere else (a
  window drag or a text selection passing over the notch) is never latched and changes nothing.
- Clicks: the panel is a native `NSPanel` created at startup with style mask `Borderless | NonactivatingPanel`, level,
  collection behavior and no shadow as above, a clear background and `opaque = false`. The Tauri window's content view
  (the webview) is moved into it, the Tauri window itself is never shown, and the webview is built with
  `accept_first_mouse(true)`. Every frame read and set (placement, hover) is the panel's. After startup no code holds
  the Tauri `WebviewWindow`: the threads hold an `AppHandle` (for `emit` and `run_on_main_thread`), not the window. Effect: a click reaches the
  page on the first press, and the frontmost app does not change (the app is never activated). Measured on macOS 27
  (click spike, 2026-10-22): a plain window, a non-activating style mask on it, or swapping its class to an `NSPanel`
  subclass after creation all activate the app on click; only a panel created as non-activating does not.
- Page load: when `$NOTCH_TRACE` is set and non-empty, the app logs `page: loaded` to stderr once the page has finished
  loading, so a check knows when the page can take clicks.
- Expanded size: width 420, height `196 + 18 + 4 + 22 * shown_rows + 14` pt, top edge at the screen top, centered on
  the notch. `shown_rows = max(1, min(total, 4) + (1 if total > 4 else 0))`: up to four session rows, a "N more" row
  when there are more, and the "No Claude Code sessions" row when there are none. The music block above the sessions
  is always there (it shows "Nothing playing" without a track), so the height does not depend on `Media`.

## 4. UI (`dist/index.html`)

The approved prototype (https://claude.ai/artifact/96cCAcQdoj6VVTnpDFEdCa) is the visual reference; this section and
section 7 "App and UI" are its static part (no motion). Colors: text `#f5f5f5`, muted `#8b8b8b`, amber `#f5a623`
(attention), green `#34d058` (working), gray `#8b8b8b` (idle); the pill is pure black. Font: the system UI font.

- Collapsed (305 x 32 on a 185 pt notch: the notch plus a 60 pt ear each side), left ear: an 8 pt dot and `total`,
  600 12 px tabular, at 0.82 opacity (1 while expanded). Dot: amber with a 6 px glow of its color when
  `attention > 0`; else green with glow when `working > 0`; else gray, no glow.
- Collapsed right ear: Now Playing only (section 7). Attention shows on the dot only; there is no `⚠ N`.
- Expanded, the session section starts 196 pt below the panel top, 16 pt from the left, 388 pt wide:
  - header row, 18 pt tall, 600 11 px muted: `Claude Code` on the left; on the right `<attention> need you`
    (`needs you` when 1) in amber when `attention > 0`, else `<working> working` when `working > 0`, else `All quiet`.
  - 4 pt gap, then one 22 pt row per session for the first four rows: an 8 pt dot (as the left ear's, by that row's
    status: amber for `needs_permission`, `needs_input`, `error`; green for `working`; gray otherwise), 8 pt gap, the
    label (500 12 px, ellipsis), and the status text right-aligned (muted; amber for `needs_permission`,
    `needs_input`, `error`; ellipsis past 170 pt). Status text: `needs permission: <detail>` (or `needs permission`),
    `needs your input`, `error`, `working`, `done`, `waiting for you`.
  - when `total > 4`, one more muted row: `<total - 4> more`. With no sessions, one muted row:
    `No Claude Code sessions`.

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

The panel shows what is playing, and its card has play/pause, previous, next and a seek bar (see Controls).

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
   `<music|spotify>\t<playing|paused|stopped>\t<title>\t<artist>\t<album>\t<duration ms>\t<position ms>\t<artwork url or empty>`.
   Positions and durations are printed as whole milliseconds (`round (x * 1000)`), so no locale decimal comma
   appears and the page does not step backward. Each `osascript` is killed after 3 s; a timeout gives health
   `failed` for that poll. Stderr containing `-1743` means Automation access was refused: health `denied`, no retry
   until the next poll.

When the adapter stream exits, the provider first reports `{via: adapter, health: failed, track: null}` so no stale
track stays on screen, then waits `backoff(attempt)` (1 s doubling, capped at 60 s; a stream that lived 60 s or more
resets `attempt` to 0 so the next wait is 1 s), runs `test` again and picks a source again.

Every adapter process (`test` and `stream`) runs in its own process group under a watchdog that holds a pipe from
the app: when the app exits for any reason, SIGKILL included, the pipe closes and the watchdog stops the adapter.
Stopping a child early (a `test` timeout, a read error, the provider unwinding) uses the same stop. A stop first
lists the adapter's direct children (the adapter's test client runs in a process group of its own, measured on
macOS 27.0.1), sends SIGTERM to the adapter's group and to those children, waits up to 5 s (longer than the
adapter's own 3 s cleanup), then sends SIGKILL to whatever of them is left. A panic inside the provider is
caught; it logs `media: Failed: <reason>` to stderr and restarts under the same backoff. When the adapter is not
usable it logs `media: adapter Failed (<reason>), using AppleScript` once per switch, polls AppleScript, and runs
`test` again every 60 s, switching back to the adapter when it passes.

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
  `com.spotify.client` / Spotify. Duration and position are milliseconds; a fractional value is rounded;
  `elapsed_at_ms` = `now_ms`; empty artist, album or artwork become `None`.
- `applescript_denied(stderr: &str) -> bool`: true when `stderr` contains `-1743`.
- `backoff(attempt: u32) -> std::time::Duration`: `min(2^attempt, 60)` seconds.

### Controls

The page sends one command, `act`, with one argument, `action`: a JSON object parsed at the boundary into a closed
enum. Nothing past `parse_action` sees raw JSON.

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase", deny_unknown_fields)]
pub enum Action {
    Toggle,                       // {"kind":"toggle"}
    Next,                         // {"kind":"next"}
    Previous,                     // {"kind":"previous"}
    Seek { position_ms: u64 },    // {"kind":"seek","position_ms":74000}
}

pub const MAX_SEEK_MS: u64 = 24 * 3600 * 1000;

/// Where an action goes: adapter arguments after the three paths, or one AppleScript for `app` ("Music" or
/// "Spotify", the process name `pgrep -x` checks).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Route {
    Adapter(Vec<String>),
    Script { app: &'static str, script: String },
}
```

- `parse_action(v: &serde_json::Value) -> Result<Action, String>`. `Err` for anything that is not one of the four
  shapes above: not an object, a missing or unknown `kind` (`"Toggle"`, `"play"` and `"pause"` included), an extra
  field, `position_ms` missing on `seek`, or a `position_ms` that is negative, fractional, a string, or greater than
  `MAX_SEEK_MS`.
- `route(media: Option<&Media>, action: Action) -> Result<Route, String>`. `Err("nothing playing")` when `media` is
  `None` or its `track` is `None`. A `Seek` position is first clamped to the track's `duration_ms` when that is known.
  Then by `media.via`:
  - `adapter`: `Toggle` `["send", "2"]` (kMRATogglePlayPause), `Next` `["send", "4"]`, `Previous` `["send", "5"]`,
    `Seek` `["seek", "<position_ms * 1000>"]` (the adapter takes microseconds).
  - `applescript`: by the track's `bundle_id`, `com.apple.Music` goes to app `Music` and `com.spotify.client` to app
    `Spotify`; any other bundle id is `Err`. The script is exactly `tell application "<app>" to <verb>` with verb
    `playpause`, `next track`, `previous track`, or `set player position to <s>` where `<s>` is the position in
    seconds written as `<position_ms / 1000>.<position_ms % 1000, three digits>` (`74000` → `74.000`; AppleScript
    source literals always use a period, whatever the locale).
  - `none`: `Err`.
- `apply(route: &Route) -> Result<(), String>` (not pure). `Adapter`: runs `/usr/bin/perl <dir>/mediaremote-adapter.pl
  <dir>/MediaRemoteAdapter.framework <dir>/MediaRemoteAdapterTestClient <args...>` with `<dir> = adapter_dir()`, under
  the same process group and watchdog as every adapter process, stdout and stderr discarded; `Ok` when it exits 0
  within 5 s, else `Err` (and it is stopped like a timed-out `test`). `Script`: `Err("<app> is not running")` without
  calling `osascript` when `pgrep -x <app>` finds nothing, so a control never launches a player; otherwise one
  `osascript` call under the 3 s timeout; `Ok` on exit 0, `Err` otherwise (stderr containing `-1743` gives
  `Err("denied")`).
- The `act` command parses, routes against the latest `Media`, and applies off the main thread, returning
  `Result<(), String>` to the page. With `$NOTCH_TRACE` set it logs one line per call: `media: act <action> ok`,
  `media: act <action> failed: <reason>`, or `media: act rejected: <reason>` when `parse_action` fails, where
  `<action>` is `toggle`, `next`, `previous` or `seek <position_ms>` (after clamping).

### App and UI

- The app keeps the latest `Media` and emits `media` with it whenever it changes; the `media` command returns it
  (`null` before the first update). When `$NOTCH_TRACE` is set and non-empty, each change is also logged to stderr as
  `media: <via> <health> <playing|paused> "<title>" (<app>)`, or `media: <via> <health> nothing` without a track.
- The expanded height does not depend on `Media` (section 3).
- Tint: one color per artwork, used by the equalizer bars, the seek fill and the card glow. It is the average color of
  the artwork's pixels (drawn into a small canvas; an https artwork is loaded with `crossOrigin = "anonymous"` for
  this), lifted toward white until `0.2126 r + 0.7152 g + 0.0722 b >= 110`. With no artwork, or one that can't be read
  (a CORS failure), the tint is `rgb(245,245,245)`. The visible artwork never depends on the tint load succeeding.
- Right ear while collapsed, in pt from the collapsed panel's left edge: nothing without a track. With a track: its
  artwork, 18 x 18 at (253, 7), radius 4 (26 x 15 at (251, 8.5), radius 3, when the artwork is at least 1.3 times
  wider than tall), at opacity 1 while playing and 0.6 while paused; then, while playing, four tinted equalizer bars
  (3 pt wide, 14 pt tall, 2 pt gaps, scaling from the bottom) starting at x 278 (282 when wide), top 9; while paused,
  a 12 pt muted pause glyph at x 280 (284 when wide), top 10. With `prefers-reduced-motion`, the bars hold still.
  While expanded, the right ear shows nothing (the artwork is in the card).
- Expanded music block, in pt from the expanded panel's top-left (420 wide), always shown:
  - artwork at (32, 44), 64 x 64, radius 13, with a 1 pt inner hairline `rgba(255,255,255,0.09)`; when wide,
    112 x 63, radius 8. Without artwork (or without a track), a `#161616` placeholder with two muted rings.
  - glow: behind the artwork, a radial gradient of the tint (`rgba(tint, 0.34)` at the center, `rgba(tint, 0.08)` at
    60 %, transparent beyond), 170 x 125 pt radii centered at the artwork's center, faded out over the top 30 pt
    below the ear band so the band stays pure black. No glow without a track.
  - text at left 92, top 46, width 262 (left 142, width 232 when wide): title 600 14 px / 18 px (ellipsis; when wide,
    13.5 px / 17 px clamped to two lines); artist 500 12 px / 16 px `rgba(245,245,245,0.62)`, 2 pt below; source row
    5 pt below, 500 11 px / 14 px muted: a 5 pt tint dot and the app name.
  - while playing, four tinted equalizer bars at (384, 50), as the ear's.
  - seek row at (16, 116), 388 wide, 16 tall, three columns `46px 1fr 46px` with 6 pt gaps: elapsed, the bar,
    `-remaining` (right aligned), 500 11 px muted tabular figures (`h:mm:ss` from one hour). The bar's hit area is the
    full 16 pt row; it draws a 4 pt track (`rgba(245,245,245,0.16)`, radius 2, at 6 pt from the row top) with a fill
    of `color-mix(in srgb, <tint> 55%, #fff)`, and while dragging the track scales to 1.5x its height and a 10 pt
    white knob shows at the position. The page advances the time only while expanded and playing:
    `elapsed_ms + (now - elapsed_at_ms)`, clamped to `duration_ms`.
  - controls row at (0, 142), 420 wide, 40 tall, centered, 18 pt gaps: previous (34 pt circle, transparent, opacity
    0.86), play/pause (40 pt circle, `#f5f5f5` background, black glyph: pause while playing, play while paused), next
    (34 pt circle). Glyphs are 16 pt SVGs. `aria-label`s: `Previous track`, `Pause` or `Play`, `Next track`.
  - without a track the block keeps its place: placeholder artwork, the title `Nothing playing` and, as the artist
    line (wrapping, 15 px line height), `Play something in Apple Music or Spotify and it shows up here.`; no source
    row, no bars, no glow; the seek row is hidden; the three buttons are disabled (opacity 0.3; play/pause on a
    `#2a2a2a` circle with a `#777` glyph) and send nothing. With `health` `denied` and no track, the title is
    `Music access is off` and the artist line `Turn on notch-hud in System Settings, Privacy & Security, Automation.`
- Control behavior (the page calls `invoke('act', { action })`):
  - Play/pause sends `{kind: "toggle"}` and flips the shown state at once (when pausing, the shown position freezes at
    the current elapsed time).
  - Next sends `{kind: "next"}`.
  - Previous sends `{kind: "seek", position_ms: 0}` when the shown elapsed time is over 3 s (restart the track, as the
    prototype does), else `{kind: "previous"}`. The seek case shows position 0 at once.
  - Seek: a press on the bar starts a drag (pointer capture); while dragging, the knob, fill and times follow the
    pointer, clamped to the bar, and nothing is sent; on release the page sends one `{kind: "seek", position_ms:
    round(fraction * duration_ms)}` and shows that position at once. A `media` event during a drag does not move the
    bar. If the panel collapses during a drag, the drag is cancelled and nothing is sent. With `duration_ms` null the
    bar takes no presses.
  - Optimistic changes (toggle, the restart seek, a released drag) are pending until one of: a `media` event that
    agrees with them (toggle: its `playing` equals the shown `playing`; seek: its elapsed time at arrival is within
    2 s of the shown one), the `act` call that made
    that change rejecting (a late rejection of an earlier call leaves a newer change alone), or a deadline passing: 2 s
    when the shown media came through the adapter (it reports within about 100 ms), 5 s when it came through
    AppleScript (one poll period is 2 s plus up to two `osascript` calls). Until then a `media` event that disagrees is recorded but
    not shown, so a poll that started before the click does not flip the card back. When the pending change ends, the
    page shows the latest `media` it received. A `media` event with no track, or a different title, always shows at
    once and ends any pending change.
  - The play/pause glyphs are static markup switched by a class, so re-rendering never replaces the element under a
    press.
