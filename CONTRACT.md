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
- `inbox` and `host` come from the hook's environment, not its input, and follow `ts`. Each is a string, or `null`
  when its variable is unset or empty. Both go to `jq` through `--arg`, so no value is interpreted by the shell.
  Records written before these fields existed lack both keys; readers treat a missing key as `null`.
  - `inbox` is `$CLAUDE_CODE_MESSAGING_SOCKET`, the path of the session's inbox socket. Documented as exported to
    hooks; measured in a CLI session's hook environment and in a Desktop session's own environment. A `--restricted`
    session binds no inbox (measured), nor does a `--bare` one (docs), so their records have `inbox: null`. Socket
    names are `<pid>.sock` (measured, undocumented) and a socket file can outlive its session, so a path alone
    doesn't prove the session is live.
  - `host` is `$__CFBundleIdentifier`, the bundle id of the app the session runs in. Measured
    `com.anthropic.claudefordesktop` in a Desktop session's Bash environment. Present in hook environments and in
    terminal sessions (the terminal's bundle id) [unverified].
  - `CLAUDE_CODE_MESSAGING_TOKEN` is never recorded.

```json
{"session_id":"abc123","agent_id":null,"cwd":"/Users/lem/bianki","event":"Stop","notification_type":null,"tool_name":null,"ts":1792560000,"inbox":"/tmp/cc-socks/41234.sock","host":"com.anthropic.claudefordesktop"}
```

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
    #[serde(default)]
    pub inbox: Option<String>,
    #[serde(default)]
    pub host: Option<String>,
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
pub const MAX_ROWS: usize = 4;
pub const MAX_FILES: usize = 50;

pub fn status_of(r: &Record) -> Option<Status>;
pub fn summarize(records: &[Record], now: u64) -> Summary;
pub fn next_wake(records: &[Record], now: u64) -> Option<u64>;
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
3. `total` = session count. `attention` = sessions that are `NeedsPermission`, `NeedsInput` or `Error`. `working` = sessions that
   are `Working` (after decay).
4. `label` = last non-empty `/`-separated component of the session's `cwd`; `cwd` itself if there is none. When two or
   more sessions share a label, each of them becomes `"<label> · <first 4 chars of session_id>"`.
5. Sort sessions by status priority (`NeedsPermission` 0, `NeedsInput` 1, `Error` 2, `Working` 3, `Done` 4, `Waiting` 5),
   then `ts` descending, then `session_id` ascending. Then truncate `rows` to `MAX_ROWS`.

### `next_wake(records, now)`

The earliest unix second after `now` at which `summarize(records, t)` can differ from `summarize(records, now)` with
no change to `records`: the smallest of

- `r.ts + TTL_SECS + 1` for every record that step 1 of `summarize` keeps at `now` (the second its age passes
  `TTL_SECS` and it drops out);
- `ts + QUIET_SECS + 1` for every session (after superseding, step 2) whose chosen status is `Working` and has not
  decayed at `now`, where `ts` is the session's largest `ts` (the second it decays to `Waiting`).

`None` when there is neither (no record kept, and no `Working` session left to decay). Every value it returns is
greater than `now`. Pure: it reads no clock and no files.

### `read_dir(dir)`

Considers only regular files whose names end in `.json`. Keeps the `MAX_FILES` most recently modified (mtime
descending, file name ascending on ties), parses each as a `Record`, and skips any file that fails to read or parse.
A missing directory returns an empty list. Never panics.

### `prune(dir, now)`

Best effort, never panics: deletes every `*.json` file in `dir` whose mtime is more than `TTL_SECS` before `now`.
The app calls it before each `read_dir`.

### `sessions_dir()`

`$NOTCH_DIR` if set and non-empty, else `$HOME/.claude/notch/sessions`.

### Session ids and `detail` (amendment, 2026-10-22)

This amends the model above; where it differs, it wins.

```rust
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Row { pub id: String, pub label: String, pub status: Status, pub detail: Option<String> }

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

pub fn valid_id(id: &str) -> bool;
pub fn detail(records: &[Record], id: &str, now: u64) -> Option<Detail>;
```

- `Row.id` is the session's `session_id`.
- `valid_id`: the hook's rule (section 1, step 1): non-empty and entirely `[A-Za-z0-9_-]`, anchored at both ends, so
  a trailing newline is invalid.
- `detail(records, id, now)`: `None` when no session that `summarize(records, now)` derives (steps 1 and 2, before
  rows are truncated to `MAX_ROWS`) has `session_id == id`. Otherwise, for that session after superseding:
  - `id`, `cwd` and `status` (after quiet decay) as `summarize` derives them;
  - `label` as step 4 derives it among all sessions, so it equals the session's `Row.label` whenever it has a row;
  - `event`, `event_ts` and `tool_name` from the newest record left in the group: the largest `ts`; on a tie, the
    main-thread record, then the smallest `agent_id`;
  - `agents`: how many records left in the group have `agent_id` set.

  Pure: it reads no clock and no files.

## 3. App behavior

- Startup: when no screen in `NSScreen::screens` has `safeAreaInsets().top > 0`, print `notch-hud: no notched display`
  to stderr and exit 0. Set window level, collection behavior and shadow once.
- Every time the window is placed (startup, a `surface` call, a layout change), look the notched screen up again on the
  main thread, so plugging in or rearranging displays moves the panel correctly. If none is found, leave the frame.
- Sessions refresh: `prune(&dir, now)`, then `records = read_dir(&dir)` and `summarize(&records, now)` with
  `dir = sessions_dir()`. When the summary differs from the last one, emit `sessions` with the `Summary` as payload
  and, when `$NOTCH_TRACE` is set and non-empty, log `sessions: ` followed by the summary as one line of JSON to
  stderr. After every refresh, on the main thread: compute the frame the panel should have now (notched screen looked
  up again, current window mode, section 8) and set it when it differs from the window's frame, so a display change
  moves the collapsed pill too.
- Refreshes are driven by events, not a fixed poll. One thread runs a refresh at startup and then waits for the first
  of:
  1. a file-system event in `dir` (FSEvents through the `notify` crate, non-recursive). Events that arrive while a
     refresh runs are drained before the next one, so a burst (the hook's temp file and `mv`) costs one or two
     refreshes, not one per event. A hook write reaches the `sessions` event within 300 ms;
  2. the deadline `next_wake(&records, now)` from the last refresh, so a decay (`Working` to `Waiting`) or a TTL
     expiry shows with no file event, within 1 s of the second it is due;
  3. the rescan: `RESCAN_SECS` = 5 s after the last refresh, the safety net for a missed event, a directory that
     was deleted and recreated, and a display change.
  At startup the thread creates `dir` (`create_dir_all`, errors ignored) so the watch has something to attach to.
  When the watch cannot be set up, it is retried on every refresh (logging `watch: failed: <error>` under
  `$NOTCH_TRACE`), and the rescan keeps the panel at most 5 s behind. FSEvents watches the path, so a directory that
  is deleted and recreated keeps delivering events (measured 2026-10-22: a hook write after `rm -rf` of the directory
  refreshed in 35 ms). With no session activity the thread wakes once per rescan, never once per second.
- Refresh trace: when `$NOTCH_TRACE` is set and non-empty, every refresh first logs `refresh: <cause>` to stderr,
  where cause is `start` (the first refresh), `event` (a file event), `deadline` (the `next_wake` deadline) or
  `rescan`.
- Hover: driven by events, with no timer. An `NSTrackingArea` on the panel's content view, options
  `MouseEnteredAndExited | MouseMoved | ActiveAlways | EnabledDuringMouseDrag`, covers exactly the hit rect: the shape
  the page last reported with `surface` (section 8), centered on the panel with its top edge at the panel's top. A
  local event monitor for `LeftMouseDown | LeftMouseUp` sees presses on the panel. `hover` (bool) is emitted when the
  state below changes, evaluated once per input (`Hover::step` in `hover.rs`):
  - inputs: the area's `mouseEntered:`, `mouseMoved:` and `mouseExited:`, the monitor's down and up, and every
    placement (`place`: startup, each `surface` call, each sessions refresh and so each rescan). The monitor runs
    before AppKit dispatches the event; a down is applied there, an up on the next main run-loop pass
    (`performSelectorOnMainThread:` with `waitUntilDone: false`), after the webview has passed the mouse-up on. So the
    page gets `pointerup` before the `hover` out a release outside emits. Applied in the monitor, the out came first
    and the page ended the seek drag without seeking (measured 2026-10-22: `tests/controls_test.sh` overshoot failed).
  - over: `true` on entered, `false` on exited, and on a moved, a placement, a down or an up whether `hit_contains`
    holds for `NSEvent::mouseLocation()` against the hit rect (a superset of AppKit's half-open rect, so a moved from
    the current area always counts as over).
  - moved: on entered or moved, whether the cursor differs from where it was at the previous entered, moved or
    exited; `false` for any other input. So a shape growing under a still cursor (a banner opening where the cursor rests) is not a hover,
    and moving within it afterwards is.
  - in: `next_hover(was_in, over, moved)` (`layout.rs`): it becomes out on any input with over `false`.
  - A press the monitor sees while the cursor is in and the hit rect is not `pill` is latched until the monitor's up
    is applied; while it is latched `hover` stays true even when the cursor leaves, so a seek drag that overshoots the
    bar does not collapse the panel under the pointer. No other input unlatches: `NSEvent::pressedMouseButtons()` is
    the hardware state, already up before AppKit dispatches the mouse-up, so unlatching on it could still send the
    out ahead of `pointerup`. As a net for an up the monitor never saw, the deferred up is scheduled by the second
    placement in a row that finds the latch set and the left button up (`pressedMouseButtons() & 1 == 0`); a real
    up unlatches before the next placement, so the net never runs ahead of one. A missed up lasts at most two
    rescans (10 s). A press that starts anywhere
    else (a window drag or a text selection passing over the notch) never reaches the monitor, is never latched and
    changes nothing.
  - An entered or exited whose `trackingArea` is not the current area (queued before a placement replaced it) is
    applied as a moved, so it is judged against the current hit rect: it neither sets in outside the rect nor is
    lost when the cursor came to rest inside it. A moved carries no area: `-[NSEvent trackingArea]` raises for any
    other event type, and an Objective-C exception there aborts the app (measured 2026-10-22: the motion check's 1 pt
    move crashed it).
  - After every placement and every up, the area is replaced by one for the current hit rect and panel frame, added
    with `AssumeInside` exactly when over was `true` at that input, so AppKit's idea of inside matches the state and
    an area appearing under a still cursor sends no `mouseEntered:`. The rescan places the panel every 5 s, so a
    state that drifted from the cursor (a missed exited) lasts at most 5 s; the poll corrected it within 50 ms.
  Measured in a spike (2026-10-22, macOS 27, `NSPanel` with a `WKWebView` in an accessory app, Firefox frontmost):
  the area fires entered, moved and exited while the panel is not key and the app is not active, and the frontmost
  app does not change; a `CGWarpMouseCursorPosition` warp also fires entered and exited; the screen's top row counts
  as inside; an area added under a still cursor without `AssumeInside` sends `mouseEntered:` at once and no
  `mouseMoved:` for a following 1 pt move, while with it a 1 pt move sends `mouseMoved:`; a drag from the area to a
  point outside it but inside the window sends no `mouseExited:`, which is why the area is replaced after every up.
- Clicks: the panel is a native `NSPanel` created at startup with style mask `Borderless | NonactivatingPanel`, level,
  collection behavior and no shadow as above, a clear background and `opaque = false`. The Tauri window's content view
  (the webview) is moved into it, the Tauri window itself is never shown, and the webview is built with
  `accept_first_mouse(true)`. Every frame read and set (placement, hover, the tracking area) is the panel's. After startup no code holds
  the Tauri `WebviewWindow`: the threads hold an `AppHandle` (for `emit` and `run_on_main_thread`), not the window. Effect: a click reaches the
  page on the first press, and the frontmost app does not change (the app is never activated). Measured on macOS 27
  (click spike, 2026-10-22): a plain window, a non-activating style mask on it, or swapping its class to an `NSPanel`
  subclass after creation all activate the app on click; only a panel created as non-activating does not.
- Page load: when `$NOTCH_TRACE` is set and non-empty, the app logs `page: loaded` to stderr once the page has finished
  loading, so a check knows when the page can take clicks.
- Expanded size: width 420, height `196 + 18 + 4 + 22 * shown_rows + 14` pt, top edge at the screen top, centered on
  the notch. `shown_rows = max(1, rows.len() + (1 if total > rows.len() else 0))`: the session rows (`MAX_ROWS`, 4, at
  most), a "N more" row when `total` is larger, and the "No Claude Code sessions" row when there are none. The music block above the sessions
  is always there (it shows "Nothing playing" without a track), so the height does not depend on `Media`. This is the `open`
  shape of section 8's `Layout`.
- Session detail (amendment, 2026-10-22): every sessions refresh also keeps the `records` it read and its `now`.
  Command `session_detail(id: String) -> Option<Detail>`: when `valid_id(&id)` is false it returns `None` and, when
  `$NOTCH_TRACE` is set and non-empty, logs `page: detail rejected` to stderr. Otherwise it returns
  `detail(&records, &id, now)` with the records and `now` of the last refresh (`None` before the first), so the card
  agrees with the list it was opened from, and logs `page: detail <id> found` (or `none` when it returns `None`) under
  `$NOTCH_TRACE`.
- The refresh stores its records and `now` before it emits anything. When the records differ from the last refresh's,
  it emits `records` (no payload) after `sessions`: the card shows the newest event and the subagents, which can change
  while the summary does not (a session that stays `Working` from one tool to the next).
- Key focus (amendment, 2026-10-23): the panel is `NotchHudPanel` (`src-tauri/src/panel.rs`), an `NSPanel` subclass
  still created `Borderless | NonactivatingPanel` as in "Clicks". Its `canBecomeKeyWindow` returns a main-thread flag,
  `KEY_WANTED`, which is `false` at startup and stays `false` until `focus(true)`. While it is `false` the panel never
  becomes key and clicks behave exactly as "Clicks" says.
  Command `focus(on: bool)`, synchronous like `surface`, so it runs on the main thread in arrival order:
  - `on`: set the flag, `makeKeyWindow`, then `makeFirstResponder` with the webview (the one subview of the content
    view wry builds). When the panel is still not key afterwards, the flag is cleared again. Under `$NOTCH_TRACE`, log
    `key: on` to stderr when the panel was not key before the call and is key after it.
  - off: clear the flag, then `NSApp.deactivate()`. Nothing calls `resignKeyWindow`.
  - off, then (amendment, 2026-10-26): `orderOut` and `orderFrontRegardless` on the panel. After a deactivate,
    AppKit's mouse-down handling makes a panel that was key key again through `_NSShowNonActivatingKeyWindow`,
    which never asks `canBecomeKeyWindow`; ordered out and back in, the next click goes through
    `_changeKeyAndMainLimitedOK:`, which asks, so the flag holds (call stacks measured in a spike). The panel is
    key only between `focus(true)` and the next give-back or resign.
  - The panel observes its own `NSWindowDidResignKeyNotification`. On every resign (`focus(false)`, a click in another
    app) it clears the flag, emits `key` with `false` and, under `$NOTCH_TRACE`, logs `key: off` to stderr.
  - No code reads `NSApp.isActive`: it reads `true` while the panel is key, though the frontmost app has not changed.
  - Nothing in the page calls `focus` yet; the prompt box (`docs/PLAN-video-sessions.md` Phase 3) is the first caller.
  Measured in the planning spike (`docs/PLAN-video-sessions.md` section 3.2; macOS 27, an accessory `LSUIElement`
  bundle opened with `open -g`, a WKWebView with an `<input>` in a `Borderless | NonactivatingPanel` subclass):
  - flag off, `orderFrontRegardless`: the panel is not key, the app is not active, the frontmost app is unchanged;
  - flag on, `makeKey`, `makeFirstResponder(webview)`: the panel is key and the frontmost app is unchanged, while
    `NSApp.isActive` reads `true`;
  - key events dispatched in-process (`NSApp.sendEvent`) reached the `<input>`;
  - giving key back with `NSApp.deactivate()`, or with `orderOut` + `orderFrontRegardless`, left the panel not key and
    the app inactive at once and 3 s later, frontmost unchanged;
  - `resignKey()` reported not key at once, but the panel was key again 3 s later (Apple says never to call it);
  - a plain borderless `NSPanel` stays not key after `makeKey`.
  Not measured yet: real keystrokes from the keyboard, whether the previous app's window gets keyboard focus back after
  the give-back, and ⌘V/⌘A in the field. The Phase 3 check covers them.

## 4. UI (`dist/index.html`)

The approved prototype (https://claude.ai/artifact/96cCAcQdoj6VVTnpDFEdCa) is the visual reference; this section and
section 7 "App and UI" are its static part (no motion). Colors: text `#f5f5f5`, muted `#8b8b8b`, amber `#f5a623`
(attention), green `#34d058` (working), gray `#8b8b8b` (idle); the pill is pure black. Font: the system UI font.

- Collapsed (305 x 32 on a 185 pt notch: the notch plus a 60 pt ear each side), left ear: the avatar (section 10)
  and `total`, 600 12 px tabular, at 0.82 opacity (1 while expanded). `total` is amber when `attention > 0`, else
  `#f5f5f5`. Only when the avatar cannot be shown (section 10, "Page") does the 8 pt dot stand in its place: amber
  with a 6 px glow of its color when `attention > 0`; else green with glow when `working > 0`; else gray, no glow.
- Collapsed right ear: Now Playing only (section 7). Attention shows in the left ear only (the amber count and the
  avatar's mood, section 10); there is no `⚠ N`.
- Expanded, the session section starts 196 pt below the panel top, 16 pt from the left, 388 pt wide:
  - header row, 18 pt tall, 600 11 px muted: `Claude Code` on the left; on the right `<attention> need you`
    (`needs you` when 1) in amber when `attention > 0`, else `<working> working` when `working > 0`, else `All quiet`.
  - 4 pt gap, then one 22 pt row per entry of `rows`: an 8 pt dot (as the left ear's, by that row's
    status: amber for `needs_permission`, `needs_input`, `error`; green for `working`; gray otherwise), 8 pt gap, the
    label (500 12 px, ellipsis), and the status text right-aligned (muted; amber for `needs_permission`,
    `needs_input`, `error`; ellipsis past 170 pt). Status text: `needs permission: <detail>` (or `needs permission`),
    `needs your input`, `error`, `working`, `done`, `waiting for you`.
  - when `total > rows.length`, one more muted row: `<total - rows.length> more`. With no sessions, one muted row:
    `No Claude Code sessions`.

### Detail card (amendment, 2026-10-22)

Visual reference: the [Sessions and Video prototype](https://claude.ai/artifact/W6jgY4nGA9NwnY7xYCbAcQ). This phase
has no prompt box: its slot stays empty.

- Each entry of `rows` is drawn as `<button type="button" class="srow" data-id="<row.id>">` with the content above,
  no border, a transparent background, radius 6, and 6 pt of horizontal padding offset by -6 pt margins, so the
  content keeps its place. The `N more` and `No Claude Code sessions` rows are not buttons.
- A click on a row button calls `invoke('session_detail', { id })`. When the reply is not null and no other row or
  the back control was clicked since, the session section is replaced by the card. A null reply leaves the list.
- The card starts where the session section does (196 pt from the top, 16 pt in, 388 wide):
  - header, 18 pt: a back control, `<button type="button" class="back" aria-label="Back to all sessions">`, with a
    10 pt chevron and `Claude Code` (600 11 px muted), then the detail's `label` (600 11 px `#f5f5f5`, ellipsis);
  - 4 pt, then four 22 pt rows: a 78 pt name column (500 11.5 px muted) and the value (500 12 px, ellipsis):

    | Name | Value |
    |---|---|
    | `Status` | an 8 pt dot as the list row's, then the status text above, without a tool (`needs permission`); amber for `needs_permission`, `needs_input`, `error` |
    | `Folder` | `cwd`, with a leading `/Users/<name>` shown as `~` (not `/Users/Shared`) |
    | `Last event` | the event in words (below), then its age, muted tabular figures |
    | `Subagents` | `<agents> running`, or `None` when 0 |

  - Event words, with `<tool>` the `tool_name` or `a tool` when null: `SessionStart` `Started`; `UserPromptSubmit`
    `Got a prompt`; `PreToolUse` `Running <tool>`; `PostToolUse` `<tool> finished`; `PostToolUseFailure`
    `<tool> failed`; `PermissionRequest` `Asked to use <tool>`; `PermissionDenied` `Permission denied`;
    `Notification` `Waiting for your answer`; `Stop` `Finished its turn`; `StopFailure` `Stopped with an error`; any
    other event, its name.
  - Age from `event_ts` and the page's clock: `<s> s ago` under 60 s, `<m> min ago` under an hour, else `<h> h ago`
    (whole numbers, rounded down; a future `event_ts` is `0 s ago`). It is redrawn every second while the card is
    shown and the phase is `expanded`.
- While the card is shown, every `sessions` and every `records` event calls `session_detail` again with its id and
  redraws the card from the reply. Such an event while a click's first reply is still pending sends that request again, and only
  the reply to the latest request counts. A null reply replaces the four rows with one muted 88 pt block reading `Session ended` (500 12 px); the
  header keeps the back control and the last label. A later reply that is not null shows the rows again.
- The back control shows the list. When the phase becomes `collapsed`, the list shows again, so the next expand
  starts on the list.
- The list and the card crossfade in 120 ms linear (none with reduced motion).

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

- The binary is `$NOTCH_HUD_BIN` if set, else the executable of this repo's app bundle (section 9),
  `src-tauri/target/notch-hud.app/Contents/MacOS/notch-hud`, as an absolute path. When it is not an executable file,
  print `notch-hud: build first: ./scripts/build-app.sh` to stderr, exit 1, write nothing, and call `launchctl` not
  at all.
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
   `MediaRemoteAdapterTestClient`. That directory is `$NOTCH_MEDIA_ADAPTER` if set and non-empty. Otherwise it comes
   from the running binary's path: inside an app bundle (the binary's directory is `MacOS` and that directory's
   parent is `Contents`) it is `Contents/Resources/adapter`; for any other path it is `../adapter` next to the binary
   (`src-tauri/target/adapter` for both build profiles). The choice is by path shape only, never by what exists on
   disk, so a bundle without its adapter falls back to AppleScript rather than to a dev build's adapter. It reports any app macOS lists
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
- Tint: one color per artwork, used by the equalizer bars, the seek fill and the card glow. It is the alpha-weighted
  average color of the artwork's pixels (drawn into a small canvas: a data URL from the loaded visible image, an https
  artwork through a second image loaded with `crossOrigin = "anonymous"`), lifted toward white until
  `0.2126 r + 0.7152 g + 0.0722 b >= 110`. While a new artwork's tint is being read, the previous tint stays (no flash
  to white). With no artwork, one that fails to load, one that can't be read (a CORS failure) or one with no opaque
  pixels, the tint is `rgb(245,245,245)`. The visible artwork never waits on the tint.
- Right ear while collapsed, in pt from the collapsed panel's left edge: nothing without a track. With a track: its
  artwork, 18 x 18 at (253, 7), radius 4 (26 x 15 at (251, 8.5), radius 3, when the artwork is at least 1.3 times
  wider than tall), at opacity 1 while playing and 0.6 while paused; then, while playing, four tinted equalizer bars
  (3 pt wide, 14 pt tall, 2 pt gaps, scaling from the bottom) starting at x 278 (282 when wide), top 9; while paused,
  a 12 pt muted pause glyph at x 280 (284 when wide), top 10. With `prefers-reduced-motion`, the bars hold still.
  While expanded, the right ear shows nothing (the artwork moves into the card, section 8).
- Expanded music block, in pt from the expanded panel's top-left (420 wide), always shown:
  - artwork at (16, 44), 64 x 64, radius 13, with a 1 pt inner hairline `rgba(255,255,255,0.09)`; when wide,
    112 x 63, radius 8. Without artwork (or without a track), a `#161616` placeholder with two muted rings.
  - glow: behind the artwork, a radial gradient of the tint (`rgba(tint, 0.34)` at the center, `rgba(tint, 0.08)` at
    60 %, transparent beyond), 170 x 125 pt radii centered at the artwork's center ((48, 76), or (72, 75.5) when
    wide), faded out over the top 30 pt
    below the ear band so the band stays pure black. No glow without a track, or while the placeholder shows.
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

## 8. Panel motion and window

The approved prototype is the spec for look and timing; its motion code is ported. The page animates one black shape
by its `clip-path` inside a window that Rust grows before the shape needs the room and shrinks only after the shape is
back to the pill. Only `transform`, `opacity` and `clip-path` animate; nothing animates layout or the window frame.

### Geometry (`src-tauri/src/layout.rs`, pure)

```rust
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize)]
pub struct Size { pub width: f64, pub height: f64 }

#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize)]
pub struct Layout { pub stage: Size, pub pill: Size, pub peek: Size, pub banner: Size, pub open: Size, pub ear: f64 }

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Shape { Pill, Peek, Banner, Open }       // the hit rect

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WindowMode { Pill, Banner, Open }

pub const EAR: f64 = 60.0;
pub const EXPANDED_WIDTH: f64 = 420.0;
pub const BANNER_WIDTH: f64 = 380.0;
pub const MARGIN: f64 = 16.0;

pub fn layout(notch_width: f64, notch_height: f64, screen_height: f64, rows: usize, total: usize) -> Layout;
pub fn expanded_height(rows: usize, total: usize) -> f64;
pub fn shape_size(layout: &Layout, shape: Shape) -> Size;
pub fn window_size(layout: &Layout, mode: WindowMode) -> Size;

/// What the page last asked for. `Surface::new()` is (Pill, Pill).
pub struct Surface { pub window: WindowMode, pub hit: Shape, /* private: the open-height floor */ }
impl Surface {
    pub const fn new() -> Surface;
    pub fn set(&mut self, window: WindowMode, hit: Shape);
    pub fn window_size(&mut self, layout: &Layout) -> Size;
}

pub fn hit_contains(size: Size, center_x: f64, top: f64, x: f64, y: f64) -> bool;
pub fn next_hover(was_in: bool, over: bool, moved: bool) -> bool;
```

- `layout`: `pill` = (`notch_width + 2 * EAR`, `notch_height`); `peek` = `pill` + 4 in both; `banner` =
  (`BANNER_WIDTH`, `notch_height + 46`); `open` = (`EXPANDED_WIDTH`, `expanded_height(rows, total)`), the section 3
  height for the summary's row count and total; `stage` = (`EXPANDED_WIDTH + 2 * MARGIN`, `screen_height`); `ear` =
  `EAR`.
- `shape_size` returns the matching field. `window_size`: `Pill` is `pill` exactly; `Banner` is (`banner.width + 2 *
  MARGIN`, `banner.height + MARGIN`); `Open` is (`stage.width`, `open.height + MARGIN`). The margin leaves room for
  the spring's overshoot.
- Every rect (shape, hit, window) is centered horizontally on the notch's center with its top edge at the screen top.
- `Surface::window_size`: `window_size(layout, self.window)`, except that while the mode stays `Open` the height never
  drops below the tallest `Open` height it returned since the mode became `Open`, so a row disappearing mid-spring
  never cuts the shape. `set` with a different window mode resets that floor; `set` with the same mode keeps it.
- `hit_contains`: the point (AppKit coordinates, y up) is inside the rect of `size` centered on `center_x` whose top
  edge is `top`: `|x - center_x| <= width / 2` and `top - height <= y <= top`, edges included.
- `next_hover`: `false` when not `over`; else `was_in || moved` (section 3).

### Hover (`src-tauri/src/hover.rs`, pure)

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Input { Entered, Moved, Exited, Down, Up, Placed }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Hover { /* private: in, latched, the last value returned */ }

impl Hover {
    pub const fn new() -> Hover;
    pub fn step(&mut self, input: Input, contains: bool, moved: bool, hit: Shape) -> Option<bool>;
    pub fn latched(&self) -> bool;
}
```

- `Hover::new()`: out, not latched, last value `false`. `latched()`: whether a press is latched now.
- `step` applies one input (section 3). `contains`: `hit_contains` at the cursor; `moved`: the cursor differs from
  where it was at the previous pointer input; `hit`: the current hit shape. In order:
  1. over: `true` for `Entered`, `false` for `Exited`, else `contains`.
  2. in = `next_hover(in, over, moved && input is Entered or Moved)`.
  3. latch: `Down` while in and `hit != Pill` latches; `Up` unlatches; no other input changes it.
  4. the value is `in || latched`. Returns `Some(value)` when it differs from the last value returned (or from
     `false` before the first), else `None`.

### Bridge

- Command `layout` returns the current `Layout`. The app emits `layout` with it whenever it changes (the summary's
  row count or total, or the notched screen's geometry).
- Command `surface` takes `{ window: WindowMode, hit: Shape }` (lowercase strings; an unknown value is rejected with
  `Err`). It is a synchronous command, so Tauri runs it on the main thread in the order the calls arrive: it records
  both with `Surface::set` and places the panel before it returns, so the page can wait for the bigger window. The
  page is its only caller.
- Window frame: `Surface::window_size(layout)`, set only when it differs from the panel's frame.
- Trace (`$NOTCH_TRACE`): `window: <pill|banner|open> <width>x<height>` each time the frame's size changes, and
  `hover: in` / `hover: out` each time `hover` is emitted.

### Page structure (`dist/index.html`)

- `#stage`: `stage.width` × `stage.height`, at `left: calc(50% - stage.width / 2)`, `top: 0`, so it stays put on screen
  whatever the window size. `#panel`: the black shape filling the stage, clipped by
  `inset(0 L (stage.height - h) L round 0 0 r r)` with `L = (stage.width - w) / 2` for the current shape (w, h) and
  radius `r`: pill 14, peek 14, banner 20, open 22. Children of `#panel` (so the clip cuts them): `.lear` (avatar,
  dot, count), `#earBars`, `#earPause`, `#art`, `#content` (the music block and the session section of sections 4 and 7,
  positioned from its top-left as there), `#banner`. `#panel` has the `video` class while the artwork is wide.
- `body.dataset.phase` is the current phase name (below). The stage stays hidden until the first `layout` arrives.
- Positions, in stage pt, with `pillL = (stage.width - pill.width) / 2`, `R = pillL + pill.width - ear`,
  `bannerL = (stage.width - banner.width) / 2`, `contentL = (stage.width - open.width) / 2`:
  - `.lear`: the avatar and the count, placed by section 10's table ("Page").
  - `#art`: one element for every place, replacing section 7's separate ear and card artwork: a 64 × 64 box (radius
    13), or 112 × 63 (radius 8) when wide, with the hairline and, without an image, the placeholder of section 7,
    `transform-origin: 0 0`, placed by `translate(x, y) scale(s)`. The radius scales with it (about 3.7 pt in the
    ear), as in the prototype:

    | Place | Square (x, y, s) | Wide (x, y, s) |
    |---|---|---|
    | ear | `R + 8`, `(pill.height - 18) / 2`, 18/64 | `R + 6`, `(pill.height - 15) / 2`, 26/112 |
    | banner | `bannerL + 16`, `pill.height + 4`, 34/64 | same x, y, 60/112 |
    | card | `contentL + 16`, 44, 1 | same |

    It holds two image layers that crossfade in 160 ms on an artwork change. Opacity: in the ear 0 with no track, 1
    while playing and 0.6 while paused; 1 in the banner and in the card (the placeholder there without a track).
  - `#earBars`: four 3 × 14 pt bars, 2 pt gaps, at x `R + 33` (`R + 37` wide), centered vertically; each bar animates
    `scaleY` between its low value and 1, `ease-in-out infinite alternate` (durations 0.62, 0.48, 0.71, 0.55 s, lows
    0.3, 0.5, 0.22, 0.4, delays 0, -0.2, -0.35, -0.1 s) while playing, and rests at `scaleY(0.24)` otherwise.
    `#earPause`: a 12 pt muted glyph at x `R + 35`, y `(pill.height - 12) / 2` (`R + 39` wide). Exactly one of them
    is at opacity 1 while the artwork is in the ear with a track; both are at 0 otherwise. They crossfade in 160 ms,
    delayed 120 ms when appearing from neither. The bars are tinted (section 7).
  - `#content`: x `contentL`, `open.width` wide, `open.height` tall.
  - `#banner`: `#bIcon` (34 pt, radius 9, `#171717`, a 10 pt status dot) at (`bannerL + 16`, `pill.height + 4`);
    text at x `bannerL + 62` (`bannerL + 88` with wide artwork), y `pill.height + 5`: `#bTitle` (13/17 pt, 600) over
    `#bSub` (11.5/15 pt, `rgba(245,245,245,0.62)`); `#bannerBars` at x `bannerL + 344` for songs.
- `.lear` rests at opacity 0.82 and goes to 1 while peeking or expanded ("ears brighten").

### Phases

`collapsed`, `intent`, `peek`, `expanded`, `banner`, `collapsing`. Each phase has a hit shape: `peek` → `peek`,
`expanded` → `open`, `banner` → `banner`, the rest → `pill`. The page calls `surface` whenever the pair
(window mode, hit shape) changes.

| Phase | Event | Next | What happens |
|---|---|---|---|
| collapsed, collapsing | hover in | intent | cancel the collapse; after 80 ms, peek |
| intent | hover out | collapsed | cancel; when the shape is not at the pill (it was collapsing), collapse it as from expanded |
| intent | 80 ms | peek | window `open`; ears brighten; clip to `peek` in 120 ms `cubic-bezier(0.2, 0, 0, 1)`; after 120 ms, expand |
| peek | hover out | peek | after 250 ms: collapsed, ears dim, clip to `pill` and art to the ear in 120 ms `cubic-bezier(0.2, 0, 0, 1)`, content and banner layers reset |
| peek | hover in while that 250 ms runs, or the 120 ms ends | expanded | expand |
| expanded | hover out | expanded | after 250 ms, collapse; hover in cancels it |
| banner | hover in | banner | after 80 ms, expand (window `open`) |
| banner | hover out | banner | after 250 ms, close the banner; hover in cancels it |
| banner | 2600 ms | collapsing | close the banner |
| collapsing | 80 ms | collapsing | clip to `pill` and art to the ear in 200 ms `cubic-bezier(0.4, 0, 1, 1)`, ears dim |
| collapsing | 200 ms more | collapsed | content or banner layer reset (opacity 0, `translateY(4px)`) |

- **Expand:** ears brighten; clip to `open` with the spring; art to the card with the spring; `#content` in (opacity
  0 → 1 and `translateY(4px)` → 0, 140 ms `ease-out`, 80 ms delay). From a banner, the banner layer fades out in 80 ms.
- **Collapse / close the banner:** the layer fades out (opacity, 80 ms linear), then the 200 ms close above.
- **Spring:** stiffness 380, damping 32, mass 1. With `w0 = sqrt(k/m)`, `z = c / (2 sqrt(k m))`, `a = z w0`,
  `wd = w0 sqrt(1 - z²)`, `x(t) = 1 - e^(-a t) (cos(wd t) + (a/wd) sin(wd t))` and
  `D = ln(sqrt(1 + (a/wd)²) / 0.005) / a` seconds: a CSS `linear()` of 40 points, point `i` = `x(i / 39 · D)` to
  4 decimals and point 39 = 1, lasting `round(D · 1000)` = 366 ms.
- **Layout change while expanded:** the clip springs to the new `open` height.
- **Reduced motion** (`prefers-reduced-motion: reduce`, also followed live): the clip changes at once; layers fade in
  120 ms linear with no rise and out in 120 ms linear; the ears' opacity changes in 120 ms linear; the art fades out in
  60 ms, jumps, and fades in over 60 ms; the ear glyphs crossfade in 120 ms with no delay. Phase timers are the same
  except the collapse's first step waits 120 ms and its second 120 ms.

### Window

- Grow before the shape needs the room: window `open` on entering `peek` or expanding from a banner; window `banner`
  on entering `banner`. When the window mode changes on the way in, the phase's animations (clip, art, layer, and
  the phase's own timer) start one animation frame after the `surface` call resolves, not before. A phase change
  before then drops that start. A hover change while waiting sets its timer as usual (the 250 ms leave, or the 80 ms
  expand from a banner), and the start then keeps that timer instead of its own (peek's 120 ms, the banner's
  2600 ms). When the mode is already right, they start at once. A mode counts as right only once a `surface` call
  for it has resolved: a start that follows a grow still in flight waits for that call too.
- A rejected `surface` call still lets the waiting start run (the phase never stalls); it cannot happen with the
  page's fixed strings.
- When a `surface` call rejects, the page sends the same (window, hit) pair again on its next phase change instead
  of treating it as sent.
- Shrink only after the shape is back: when the phase becomes `collapsed`, window `pill` is sent once no `clip-path`
  transition is running on `#panel`, on its `transitionend` (or at once when the clip changed without a transition).
  Re-entering before that cancels the shrink.
- Effect: collapsed, the window is the pill exactly, so menu-bar clicks outside the visible shape go to the menu bar.
  The page never activates the app, so the frontmost app never changes.

### Startup

The page asks for `layout`, `media` and `sessions` once at load. A reply is ignored when an event of the same kind
arrived first (it is older than the event), so a stale reply never counts as a change.

### Banners

Shown only while `collapsed`; an event in any other phase shows none and is not queued. Clip to `banner` with the
spring; `#banner` in like `#content`; closes after 2600 ms.

- **Song:** a `media` event with a track whose (title, artist, app) differs from the last track the page saw. The
  first track the page sees never shows one, even after media with no track: at launch the adapter reports nothing,
  then the playing track, which is no change. Playback starting from silence shows none either; the art appears in
  the ear. `#bTitle` is the title; `#bSub` is `<artist>, on <app>` (`in` instead of `on`
  when the artwork is wide), or `<app>` with no artist. The art moves to the banner with the spring and back to the
  ear on close; `#bIcon` is hidden and `#bannerBars` shown.
- **Session:** a `sessions` payload, not the first, with a row whose label was in the previous payload with a
  different status and whose status is now `needs_permission`, `needs_input`, `error` or `done`. A label missing from
  the previous payload never shows a banner, so a row that was relabeled (`· <id>` added or dropped) or that comes
  back within `MAX_ROWS` stays quiet; the cost is that a brand-new session, or one past `MAX_ROWS`, shows none.
  The first such row in payload order is shown: `#bTitle` its label, `#bSub` its status text (section 4), `#bIcon` with
  an amber dot (needs or error) or a gray one (done); `#bannerBars` hidden and the art stays in the ear.

### Detail (amendment, 2026-10-22)

This amends the geometry and phases above for the session detail card (section 4, "Detail card"); where it differs, it
wins.

```rust
pub struct Layout { pub stage: Size, pub pill: Size, pub peek: Size, pub banner: Size, pub open: Size, pub detail: Size, pub ear: f64 }
pub enum Shape { Pill, Peek, Banner, Open, Detail }   // "detail" on the wire
pub const DETAIL_HEIGHT: f64 = 380.0;
```

- `layout`: `detail` = (`EXPANDED_WIDTH`, `DETAIL_HEIGHT`). 380 = the music block (196), then the card: header 18,
  4 pt, four 22 pt rows, 10 pt, the prompt box slot 34, 6 pt, the hint line 14, 10 pt. It depends on nothing else.
- `shape_size(layout, Detail)` is `detail`.
- `window_size(layout, Open)` is (`stage.width`, `max(open.height, detail.height) + MARGIN`), replacing
  `open.height + MARGIN` above, so switching between the list and the card never resizes the window. With these
  numbers the open window is always 452 × 396. `Surface::window_size` keeps its floor rule on top of this.
- Page: while the card is shown, phase `expanded` has hit shape `detail` and its clip is the `detail` shape (radius
  22); otherwise `open` as before. Showing the card or going back while expanded springs the clip to the other shape
  (the spring of "Layout change while expanded") and calls `surface` with the new hit; the window mode stays `open`.
  A `layout` event that changes `open.height` while the card is shown moves no clip.

### Typing (amendment, 2026-10-25)

This amends the phases above for the prompt box (section 11); where it differs, it wins.

- `typing` is a substate of `expanded`, entered only from the detail card (section 11). `body.dataset.phase` stays
  `expanded`, the hit shape stays `detail` and the window mode stays `open`.
- While typing, hover out sets no timer, so the panel stays expanded with the pointer gone. Hover in still cancels a
  pending collapse.
- Typing ends on Esc, on a `key` event with `false`, on a pointer press in the page outside the box, when
  `focus(true)` returns anything but `true` or fails, when a null reply hides the box, and when the card closes (back,
  or the reset when the phase becomes `collapsed`). Ending calls `focus(false)` once. When the phase is still
  `expanded` and the latest `hover` event was `false`, it then collapses after 250 ms, as hover out does.
- Starting to type cancels a pending hover-out collapse.

## 9. App bundle (`scripts/build-app.sh`)

The login item runs a signed `.app`, so macOS permission grants (Automation for Music and Spotify) belong to a stable
code identity instead of to whichever binary was built last.

### Layout

```text
src-tauri/target/notch-hud.app/Contents/
  Info.plist                 CFBundleIdentifier dev.lem.notchhud, LSUIElement true, NSAppleEventsUsageDescription
  MacOS/notch-hud
  Resources/adapter/         mediaremote-adapter.pl, MediaRemoteAdapter.framework, MediaRemoteAdapterTestClient
```

- `CFBundleIdentifier` is `dev.lem.notchhud` (`tauri.conf.json` `identifier`). TCC grants key on it; never rename it.
- `LSUIElement` is true: no Dock icon and no menu bar, matching the accessory activation policy the app sets at
  launch.
- `NSAppleEventsUsageDescription` is a non-empty sentence saying the app reads the current track from Music and
  Spotify and sends them play, pause and skip. macOS shows it in the Automation prompt.
- `Resources/adapter/` holds exactly what `scripts/build-adapter.sh` stages in `src-tauri/target/adapter/`, with the
  framework's symlinks kept as symlinks.

### Build

`scripts/build-app.sh` takes no arguments and:

1. runs `scripts/build-adapter.sh`;
2. runs `cargo tauri build --bundles app` in `src-tauri` (Tauri CLI 2.x, `cargo install tauri-cli --version 2.12.1
   --locked`); when `cargo tauri` is missing it prints `build-app: tauri-cli not found: cargo install tauri-cli
   --version 2.12.1 --locked` to stderr and exits 1;
3. copies Tauri's `src-tauri/target/release/bundle/macos/notch-hud.app` to a temp bundle beside the final one, signs
   it, and only then swaps it into `src-tauri/target/notch-hud.app` by renames, so the login item never sees a half
   written or unsigned bundle. The running app keeps its old code until it is restarted
   (`launchctl kickstart -k gui/<uid>/com.lem.notch-hud`); the script never restarts it;
4. prints the bundle's absolute path as its last stdout line and exits 0.

Any failing step exits non-zero and leaves an existing `src-tauri/target/notch-hud.app` as it was.

### Signing

- The identity is `$NOTCH_HUD_SIGN_IDENTITY` if set and non-empty, else `notch-hud Code Signing`: the name of a
  self-signed Code Signing certificate in the login keychain (README, Install). It is a name, not a secret.
- Before building, the script checks that `security find-identity -p codesigning` lists the identity by that exact
  name. When it does not, it prints `build-app: no code signing identity "<name>" (README: Install)` to stderr and
  exits 1 before building anything.
- `NOTCH_HUD_SIGN_IDENTITY=-` signs ad hoc, for test machines with no certificate. It prints `build-app: ad hoc
  signature, permission grants will not survive a rebuild` to stderr. Ad hoc is never the default.
- Signing goes inside out with `codesign --force --sign <identity> --timestamp=none`: the framework, then the test
  client, then the app. No hardened runtime: the app is personal and not notarized, and the hardened runtime would
  need an Apple Events entitlement for the AppleScript fallback.
- The signed app passes `codesign --verify --strict <app>`.
- With a certificate, the designated requirement (`codesign -d -r- <app>`) names `identifier "dev.lem.notchhud"`
  and the certificate's hash, and nothing that changes between builds (an ad hoc requirement names the build's
  `cdhash` instead). So two builds with the same certificate print the same requirement, and TCC, which matches on
  it, keeps its grants.


## 10. Avatar

A small procedural character in the left ear acts out what the sessions are doing. Rust picks a mood from the
sessions and Now Playing and loads the avatar file; the page draws it with the vendored renderer.

### Renderer (`dist/vendor/avatar-core.js`)

- `@bible-strong/avatar-core` 0.1.0 (npm, AGPL-3.0-only), bundled by `scripts/vendor-avatar-core.sh` into one classic
  script that defines `window.AvatarCore` (esbuild, IIFE, not minified). The script downloads the pinned tarball,
  refuses it unless its sha512 equals the integrity in `vendor/VENDORED.md`, and replaces the package's one
  dependency, `ajv`, with a stub whose validator throws, so no JSON Schema validator reaches the page: Rust
  validates every definition before the page sees it (below). The script reads the bundle's expected sha256 from
  `vendor/VENDORED.md` and writes nothing when its output differs (esbuild itself is fetched by version, not by hash, so this is the check
  that catches a changed toolchain). The committed bundle's sha256 equals the one in `vendor/VENDORED.md`.
- The page uses only the package's playback and scene functions (`playAvatarAnimation`, `advanceAvatarPlayback`,
  `sampleAvatarFrame`, `renderAvatarFrame`, `renderAvatarDefinition`), never `validateAvatarDefinition` or
  `parseAvatarDefinition`.
- The repository is licensed AGPL-3.0 (`LICENSE`), because the renderer is AGPL-3.0-only.

### Avatar file (`src-tauri/src/avatar.rs`)

```rust
pub const MAX_BYTES: usize = 262_144;
pub const DEFAULT: &str = include_str!("../avatar/default.avatar.json");

pub fn avatar_path() -> std::path::PathBuf;
pub fn avatar_path_from(notch_avatar: Option<&std::ffi::OsStr>, home: Option<&std::ffi::OsStr>) -> std::path::PathBuf;
pub fn parse(text: &str) -> Result<serde_json::Value, String>;
pub fn load(path: &std::path::Path) -> Result<serde_json::Value, String>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Source { User, Default }

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Avatar { pub source: Source, pub definition: serde_json::Value }
```

- `avatar_path()` is `avatar_path_from` of the environment's `NOTCH_AVATAR` and `HOME`.
  `avatar_path_from(notch_avatar, home)` (pure): `notch_avatar` if present and non-empty, else
  `<home>/Library/Application Support/notch-hud/avatar.json` (with no `home`, that path relative to the working
  directory; launchd always sets `HOME`).
- `parse(text)` accepts exactly what avatar-core 0.1.0's `parseAvatarDefinition` accepts, and returns the parsed
  JSON, with one difference: a string holding an unpaired UTF-16 surrogate escape (`"\ud800"`), which JavaScript
  accepts, is rejected, because a Rust string cannot hold it. Numbers are read exactly (serde_json's `float_roundtrip`),
  so a bound is never passed by rounding. `Err(reason)` otherwise, where `reason` starts with the RFC 6901 pointer
  of the offending member and `: ` (`/animations/idle/steps/0/expression: unknown expression 'nap'`) when there is
  one. The rules are written out in Rust rather than run from the package's schema file by a JSON Schema crate: one
  fewer dependency for a pinned version, and `tests/avatar_schema_test.sh` checks the verdicts against avatar-core
  itself. The rules:
  1. At most `MAX_BYTES` UTF-8 bytes. Valid JSON, and no object has the same key twice.
  2. The v1 schema (`avatarDefinition.schema.json` in the package): top level `schema` = `"bible-strong/avatar-definition"`,
     `schemaVersion` = 1, optional `name` (at most 120 characters), `body`, `colors`, `expressions`, `expressionOrder`,
     `animations`, `animationOrder`, optional `standardAnimationSet` = 1, and no other member. Every object in the
     schema rejects members it does not list. Semantic keys (expression and animation names, step expressions, order
     entries) match `^[a-z][a-z0-9]*(?:-[a-z0-9]+)*$` and have at most 64 characters. Colors match `^#[0-9a-f]{6}$`.
     `expressions` has 1 to 128 members and must include `neutral`; `animations` at most 64; `expressionOrder` 1 to
     128 unique entries; `animationOrder` at most 64 unique entries; `body.nodes` at most 16, whose surface type is
     not `mickey` or `cursor`; each animation 1 to 128 steps. Numeric ranges: surface `width`, `height`, `depth`
     0.001 to 10000; `roundness`, `morphRoundness`, `tipRoundness`, `baseRoundness` 0 to 1; node `position` 3 numbers
     from -10000 to 10000, `rotation` 3 numbers from -360 to 360; every head, eye and `spacing` number -10000 to 10000;
     `perspective` 0.1 to 10; `holdMs` 100 to 60000; `transitionMs` 0 to 5000; blink `initialDelayMs` 0 to 60000,
     `minIntervalMs` and `maxIntervalMs` 250 to 120000, `durationMs` 50 to 2000. Enums: surface type `sphere`,
     `mickey`, `cursor`, `cube`, `capsule`, `cylinder`, `cone`, `diamond`; motion eyes `none`, `microSaccades`,
     `shake`; motion body `none`, `slowDrift`, `shake`; `playbackMode` `loop`, `once`, `pingPong`; `transition`
     `spring`, `smooth`, `snappy`. An expression's optional `colors` has at least one of `body`, `eyes`; an
     animation's optional `metadata` at least one of `label` (at most 120 characters), `description` (512), `group`
     (64).
  3. `expressionOrder` starts with `neutral`, and it and `animationOrder` list exactly the keys of `expressions` and
     `animations`; every blink has `minIntervalMs <= maxIntervalMs`; every step's `expression` is a key of
     `expressions`.
- `load(path)`: `Err("no file")` when nothing is at `path`; `Err("not a file")` when it is not a regular file (a
  directory, or a FIFO, which it opens without blocking; symlinks are followed); the error's own text for any other
  failure to open (permission denied, for example); `Err("larger than 262144 bytes")` when the file is larger,
  without reading more than `MAX_BYTES + 1` bytes; `Err` for a read error or bytes that are not UTF-8; otherwise
  `parse` of the contents.
- `DEFAULT` is the committed default avatar, `src-tauri/avatar/default.avatar.json`: an original character made in
  this format (not any existing character, and not the avatar lab's own). `parse(DEFAULT)` is `Ok`, and it defines the
  first animation of every row of the mood table below.
- At startup the app loads `avatar_path()` once. Success logs `avatar: loaded <path>` to stderr. A failure logs
  `avatar: <path>: <reason>, using the default` to stderr (always, not only under `$NOTCH_TRACE`) and uses `DEFAULT`.
  The command `avatar` returns the `Avatar` (`source` `user` or `default`). A changed file is read at the next
  launch.
- `notch-hud --check-avatar <path>` runs `load(path)` and exits without starting the app: on `Ok` it prints `ok` to
  stdout and exits 0; on `Err(reason)` it prints `reason` to stderr and exits 1.
- The user's file is never in the repository: `.gitignore` ignores `avatar.json` and `*.avatar.json` anywhere,
  except the default. No tracked JSON file outside `src-tauri/avatar/default.avatar.json` and
  `tests/avatar/fixtures/` has `"schema": "bible-strong/avatar-definition"`. `tests/repo_test.sh` checks both rules
  and the vendored bundle's hash with nothing but git, and runs in CI on its own.

### Mood (`src-tauri/src/avatar.rs`, pure)

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Mood { Alert, Error, Celebrate, Working, Listening, Idle, Drowsy, Sleeping }

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Signals { pub needs_you: bool, pub error: bool, pub working: bool, pub playing: bool }

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Pose { pub mood: Mood, pub animations: &'static [&'static str], pub still: bool }

pub const CELEBRATE_MS: u64 = 4_000;
pub const DROWSY_MS: u64 = 120_000;
pub const SLEEP_MS: u64 = 300_000;
pub const MAX_WAIT_MS: u64 = 60_000;

pub fn signals(summary: Option<&Summary>, media: Option<&Media>) -> Signals;
pub fn finished(prev: Option<&Summary>, next: &Summary) -> bool;
pub fn pose(mood: Mood) -> Pose;
pub fn wait_ms(at: u64, now: u64) -> u64;

pub struct Moods { /* private: celebrate_until, quiet_since, the last step's time */ }
impl Moods {
    pub fn new(now_ms: u64) -> Moods;
    pub fn step(&mut self, signals: Signals, finished: bool, now_ms: u64) -> Mood;
    pub fn next_change(&self) -> Option<u64>;
}
```

| Priority | Mood | When | `animations` (the first one the avatar has plays) | `still` |
|---|---|---|---|---|
| 1 | `alert` | a row is `needs_permission` or `needs_input` | `surprised`, `curious` | no |
| 2 | `error` | a row is `error` | `sad`, `angry` | no |
| 3 | `celebrate` | less than `CELEBRATE_MS` since a session finished | `celebrate`, `happy`, `excited` | no |
| 4 | `working` | `working > 0` | `working`, `thinking` | no |
| 5 | `listening` | a track is playing | `listening` | no |
| 6 | `idle` | quiet for less than `DROWSY_MS` | `idle` | no |
| 7 | `drowsy` | quiet for `DROWSY_MS` to `SLEEP_MS` | `drowsy`, `bored` | no |
| 8 | `sleeping` | quiet for `SLEEP_MS` or more | `sleeping` | yes |

- `signals`: `needs_you` and `error` from the summary's `rows` (the status sort puts every such session in `rows`),
  `working` = `summary.working > 0`, `playing` = the media's track is `playing`. `None` gives `false`.
- `finished(prev, next)`: a row of `next` is `done` and its label is in `prev`'s rows with a different status (the
  session banner's rule, section 8). `false` when `prev` is `None`.
- `pose(mood)`: the row's `animations` and `still`.
- `Moods::new(now)` starts quiet at `now`. `step` applies one update at `now` (milliseconds since the Unix epoch):
  1. `finished` sets the celebration to end at `now + CELEBRATE_MS`.
  2. The mood is the first row of the table whose condition holds. "Quiet" means none of rows 1 to 5 holds; the
     quiet time counts from the first step that found it quiet (or from `new`), and any step that is not quiet ends
     it. A celebration that started while a row above it held still ends at its own time.
- `next_change()`: the smallest of these that is later than the last step's `now`: the celebration's end; and,
  while quiet, the quiet start plus `DROWSY_MS` and plus `SLEEP_MS`. `None` when there is none, so a sleeping avatar
  sets no timer at all.
- `wait_ms(at, now)`: `at - now` (0 when `at` is not later), at most `MAX_WAIT_MS`.
- The app runs `Moods` on a thread of its own that waits for an update or for `wait_ms(next_change(), now)`
  milliseconds, whichever comes first, and with no `next_change()` for an update only. The cap is there because the
  deadlines are wall-clock time and the wait's timer stops while the Mac sleeps.
  Updates come from the sessions refresh (when the summary changes, with `finished` against the previous summary)
  and from Now Playing (when the media changes). When the pose changes the app emits `mood` with the `Pose`, and
  with `$NOTCH_TRACE` logs `avatar: <mood>`. The command `mood` returns the current `Pose`.

### Page

- The page asks for `avatar` and `mood` at load (section 8's startup rule applies to `mood`) and listens for `mood`.
- `#avatar`, inside `.lear`: an inline SVG, `viewBox="-150 -150 300 300"`, painted as `@bible-strong/avatar-web`
  0.1.0 paints a scene: body back paths, the head path in the body color, the two eye paths in the eye color clipped
  to the head path (opacity 0 when not visible), then body front paths.
- Places, in stage pt (section 8's names), with transform only:

  | Phases | `#avatar` box | `#total` |
  |---|---|---|
  | all but `expanded` | 24 × 24 at (`pillL + 12`, `(pill.height - 24) / 2`) | left edge at `pillL + 41`, centered on the pill's height |
  | `expanded` | 36 × 36 at (`contentL + 16`, 4) | left edge at `contentL + 58`, centered on y 22 |

  They move when `#art` moves, with its duration and easing (the spring on expand, 200 ms on collapse, 120 ms on
  un-peek). With reduced motion they jump.
- Playing a pose: the first name in `animations` that the definition has. A mood change starts it with
  `playAvatarAnimation(definition, name, now, from)` where `from` is `sampleAvatarFrame` of what is on screen, so the
  face eases out of the old pose. When the pose is `still`, when reduced motion is on (followed live), or when the
  definition has none of the names, the page paints `renderAvatarDefinition(definition, key)` once, where `key` is
  that animation's first step's expression (`neutral` when there is no animation), and sets no timer: one expression
  per mood.
- Budget: no `requestAnimationFrame` loop and no CSS animation on the avatar. The page paints a frame only when the
  picture can change: during a step's transition, during a blink, or while the shown expression's `motion` is not
  `none` (ambient motion), at most one frame per 50 ms (20 frames/s). Otherwise one timer waits for the next change:
  the end of the hold or the next blink (`blinkDueAt`). A `once` animation that ended keeps its last frame and no
  timer. Measured numbers are in `docs/PLAN.md` section 7.
- Fallback: when `window.AvatarCore` is missing, the `avatar` reply is missing or rejected, or painting throws, the
  page hides `#avatar`, shows section 4's dot in its place, and stops the avatar's timers.

## 11. Session prompt (`dist/index.html`)

The prompt box of `docs/PLAN-video-sessions.md` section 3.3, typing only: it takes the keyboard and gives it back, and
Return sends nothing. Sending (`prompt`, `Reach`, the delivery states) comes later. Visual reference: the
[Sessions and Video prototype](https://claude.ai/artifact/W6jgY4nGA9NwnY7xYCbAcQ). This amends section 4 "Detail card":
the slot below the four rows holds the box and the hint line. The box is the `focus` caller that section 3 "Key
focus" waits for.

### Box

- `#pbox`, 10 pt below the rows (at y 316 of `#content`, x 16), 388 x 34, radius 10, background `#141414`, a 1 px
  inset line `rgba(245,245,245,0.1)` that turns `rgba(245,245,245,0.34)` while typing (120 ms linear, none with
  reduced motion). 11 pt of left padding, 4 pt right, 6 pt gap. Inside:
  - `<textarea id="msg" rows="1" maxlength="8000" aria-label="Message to this session">`, 18 pt tall, 500 12.5/18 px,
    no wrapping, placeholder `Message <label>` (the detail's label) in muted;
  - `<button type="button" class="send" id="send" aria-label="Send">`, 26 pt round, `#f5f5f5` with a black up arrow;
    disabled (`#2a2a2a`, arrow `#6f6f6f`) while the draft is empty after trimming.
- `#phint`, 6 pt below the box, 388 x 14, 500 11/14 px muted, one line with an ellipsis. While typing it reads
  `Return to send, Shift+Return for a new line, Esc to stop typing`; otherwise it is empty.
- A null detail reply (`Session ended`) hides the box and the hint.
- The draft is kept when typing ends, and cleared when the card closes or shows another session.

### Typing

- A pointer press on the box (not on the send button) while the card shows a session and the phase is `expanded`
  starts typing: `invoke('focus', { on: true })`, then, once it returns `true`, DOM focus moves to `#msg`. When it
  returns `false` or fails, the panel is not key, so no key and no `key` event can reach the page: typing ends at once.
  A press on the box outside `#msg` leaves DOM focus where it is.
- While typing, keys go to the page: Return (no Shift) is the send key; Shift+Return inserts a newline; Esc ends
  typing. A keydown with `isComposing` or `keyCode` 229 belongs to an input method composition and does nothing
  (WebKit sends the Return that commits a candidate with `isComposing` false and `keyCode` 229). Section 8 "Typing"
  lists every way typing ends.
- A `key` event with `false` that arrives while a `focus(true)` call has not returned is the previous give-back's and
  is ignored. A second `false` after typing ended does nothing.
- The send key and a click on the send button do the same: when the trimmed draft is not empty, call
  `invoke('prompt_draft', { text })` with it. Typing goes on and the draft stays. A press on the send button keeps DOM
  focus in the box.

### Menu

The app's menu holds one submenu, `Edit`: Undo, Redo, a separator, Cut, Copy, Paste, Select All (Tauri's predefined
items). While the panel is key, menu key equivalents reach notch-hud, not the frontmost app: Command-V and the other
edit keys need these items, and Tauri's default menu would make Command-Q quit notch-hud, Command-H hide it and
Command-W close the panel.

### Bridge

- `focus(on: bool) -> bool` (amends section 3 "Key focus"): `on` returns whether the panel is key after the call;
  off returns `false`.
- Command `prompt_draft(text: String)`: sends nothing. Under `$NOTCH_TRACE` it logs `prompt: draft <n> chars` to
  stderr, `n` being the number of Unicode scalar values in `text` trimmed.

### Sending (amendment, 2026-10-26)

This amends section 11 above and section 2 "Session ids and `detail`"; where it differs, it wins. Return sends the
draft to the session's inbox socket (`docs/PLAN-video-sessions.md` section 3.3, Phase 5). `prompt_draft` is removed.

#### Model (`src-tauri/src/prompt.rs`, pure except `send`)

```rust
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(tag = "kind", content = "host", rename_all = "snake_case")]
pub enum Reach { Inbox, HostOnly(String), None }   // {"kind":"inbox"}, {"kind":"host_only","host":"…"}, {"kind":"none"}

pub struct Prompt { pub id: String, pub text: String }
pub enum Route { Inbox(PathBuf) }
pub enum SendError { Rejected(String), Failed(String) }

pub const MAX_CHARS: usize = 8000;
pub const SUN_PATH_MAX: usize = 104;
pub const TIMEOUT: Duration = Duration::from_secs(2);

pub fn parse_prompt(id: &str, text: &str) -> Result<Prompt, String>;
pub fn route(records: &[Record], id: &str) -> Result<Route, String>;
pub fn reach(records: &[Record], id: &str) -> Reach;
pub fn frame(text: &str) -> String;
pub fn check_owner(what: &str, owner: u32, me: u32) -> Result<(), SendError>;
pub fn send(path: &Path, line: &str) -> Result<(), SendError>;
```

- `Detail` gains `pub reach: Reach`, set to `reach(records, id)`.
- `parse_prompt(id, text)`: `Err` when `id` fails `valid_id` (section 2), or when `text` with leading and trailing
  whitespace trimmed (`str::trim`) has fewer than 1 or more than `MAX_CHARS` Unicode scalar values, or contains NUL.
  Otherwise `Prompt { id, text }` with `text` trimmed.
- `route(records, id)`: the main-thread record is the record with `session_id == id` and no `agent_id` (the largest
  `ts` if there are several). `Err` when there is none, when its `inbox` is null or empty, when the path is not
  absolute, or when it is `SUN_PATH_MAX` bytes or longer (a `sockaddr_un` path holds 103 bytes and a NUL). The socket
  is shared when a record of another session has the same `inbox` string: then `Err` unless the newest `ts` among
  this session's records with that `inbox` is greater than every such record of other sessions (a tie is refused).
  Otherwise `Route::Inbox(path)`. `records` are those of the last refresh (section 3), so each one is live: written
  within `TTL_SECS` and not ended by `SessionEnd`.
- `reach(records, id)`: `Inbox` when `route` is `Ok`; else `HostOnly(host)` when the main-thread record's `host` is a
  non-empty string; else `None`.
- `frame(text)`: exactly `{"type":"user","message":{"role":"user","content":<text>}}` followed by `\n`, `<text>` a
  JSON string (serde_json's escaping: `"`, `\`, and control characters escaped, everything else as UTF-8). Key order
  is as written. Phase 0 measured this line delivered to CLI and Desktop sessions; plain text is ignored.
- `check_owner(what, owner, me)`: `Ok` when `owner == me`, else `Rejected` naming `what` and both uids.
- `send(path, line)`: `Rejected` unless `lstat` (it does not follow a symlink) shows a socket, not a symlink, owned
  by the app's uid (`check_owner`). Then connects with `TIMEOUT` (a connect that has not finished by then is
  abandoned; it writes nothing). The connect resolves the path again, so the listener's uid (`getpeereid`) must also
  pass `check_owner`, else `Rejected` with nothing written. Then it writes `line` within one `TIMEOUT` for the whole
  line (each write's timeout is the time left), and closes. It reads nothing back: a delivered, a held and an ignored
  line all close with zero bytes back (measured). `Failed` on a connect or write error or timeout; a write failure's
  reason ends `after <sent> of <total> bytes`. A partial line has no closing brace or newline, so it is not valid
  JSON [inferred: not measured how Claude Code treats it].
- No auth line. `CLAUDE_CODE_MESSAGING_TOKEN` is never read, recorded or sent: it claims the sender is the session's
  own child, which notch-hud is not (`docs/PLAN-video-sessions.md` D4).

#### Bridge

- Command `prompt(id: String, text: String) -> Result<(), String>`, run off the main thread like `act`: `parse_prompt`,
  then `route` against the last refresh's records, then `send(path, frame(text))` on a blocking task. Under
  `$NOTCH_TRACE` it logs one line to stderr: `prompt: sent <id> <n> chars` (`n` the scalar values of the trimmed text),
  `prompt: rejected: <reason>` when `parse_prompt`, `route` or `send`'s `Rejected` refused it, or
  `prompt: failed: <reason>` for `send`'s `Failed`. `Err` carries the reason. It never logs the text.
- `prompt_draft` is gone.
- `key: on` (amends section 3 "Key focus"): logged when `focus(true)` leaves the panel key and the keyboard had been
  given back before the call (the flag was off), so a second round of typing after a send or Esc logs it again.
  Measured: once the panel has been key, a click on it makes it key and `NSApp.isActive` true before `focus(true)`
  runs, so "not key before the call" missed that round. Known defect, not fixed here: such a click anywhere on the
  panel, not only on the box, takes the keyboard without typing starting, and keys typed in another app after it are
  lost to notch-hud until a click elsewhere resigns it (measured with a TextEdit decoy).
- Fixed (amendment, 2026-10-26): `focus(false)` orders the panel out and back in after the deactivate (section 3
  "Key focus"), so a click after a give-back, by Esc, a send or a resign, leaves the panel not key, logs neither
  `key: on` nor `key: off`, and keys typed afterwards reach the frontmost app. Each give-back still logs `key: off`
  once. `tests/give_back_test.sh` checks it: type, Esc, a click on the card's Folder row, then a letter that must land
  in a TextEdit decoy; then the same after a press on the Folder row that ends typing, after a click on Back that ends
  typing (the card still closes, so the press's `click` reaches the page though the panel was ordered out and in
  during the press), and after a switch to Finder and back that resigns the panel. Measured: with the plain
  deactivate the letter after the Folder row click was lost in every run (two spike runs and the check); with the
  fix it landed in every round. Not fixed here: in some runs, with and without the fix, `hover: out` arrives while
  the pointer is on the card (once right after the box took the keyboard), so the panel collapses when typing ends.

#### Page

- The box is enabled only while the card shows a session whose `reach.kind` is `inbox`. Otherwise `#msg` is disabled
  with the placeholder `This session can't take messages` (`#6f6f6f`), the send button is disabled, a press on the box
  starts no typing, and a `reach` that changes away from `inbox` while typing ends typing. The hint reads
  `It has no inbox notch-hud can send to.` for `none`, and `It has no inbox notch-hud can send to. Reply in its own
  app.` for `host_only` (Phase 6 replaces this with an `Open <app>` button).
- The send key and a click on the send button, with a non-empty trimmed draft and no send in flight, call
  `invoke('prompt', { id, text })` with the card's id and the trimmed draft, and end typing (section 8 "Typing").
- Delivery states, shown in `#phint` in place of the typing line and kept when typing ends:

  | State | When | Hint |
  |---|---|---|
  | sending | the call is in flight; the send button is disabled | `Sending…` |
  | sent | the call resolved; the draft is cleared | `Sent. Waiting for <label> to pick it up.` |
  | picked up | while sent, a redrawn card for that id has a different `event_ts` than at the send, or `status` `working` when it was not | `<label> picked it up.` in `#f5f5f5` |
  | no reply | 10 s after sent with no pickup | `No reply yet. It may be waiting for approval in that session.` in amber |
  | failed | the call rejected; the draft stays | `This session isn't reachable.` in amber |

  `<label>` is the card's label at the send. The pickup check also runs on the card already drawn when the call
  resolves. It is weak for a busy session: any hook event after the send counts, so a session already running tools
  reads as picked up even while it holds the message.
- Starting to type again clears a picked up, no reply or failed state; a sending or sent state stays until it
  resolves. Closing the card or showing another session clears the state and its timer.
- Sending ends typing, but a send in flight keeps the card open: while the state is sending, hover out sets no
  timer. When the call resolves with the latest `hover` `false`: on success the panel collapses after 250 ms, as hover
  out does; on failure it stays open with the draft and the failed hint until the pointer comes back, and a hover out
  after that collapses it as usual. A failure while the pointer is on the panel holds nothing. The hold lasts only
  while the draft can be sent: when a redraw shows the session ended or its `reach` no longer `inbox`, the hold ends,
  and with the latest `hover` `false` the panel collapses after 250 ms.

### Opening the session's app (amendment, 2026-10-26)

This amends "Sending" above and section 2 "Session ids and `detail`"; where it differs, it wins. A session the box
can't reach but whose app is known (`Reach::HostOnly`) gets an `Open <app>` button that brings that app forward
(`docs/PLAN-video-sessions.md` section 3.3, Phase 6). It never launches an app and never types into it.

#### Model

```rust
// src-tauri/src/prompt.rs, pure
pub fn valid_bundle_id(s: &str) -> bool;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(tag = "kind", content = "name", rename_all = "snake_case")]
pub enum HostOpen { Opened(String), NotRunning }  // {"kind":"opened","name":"TextEdit"}, {"kind":"not_running"}

// src-tauri/src/host.rs, AppKit
pub fn app_name(bundle_id: &str) -> Option<String>;
pub fn open(bundle_id: &str) -> Result<HostOpen, String>;
```

- `valid_bundle_id(s)`: `true` when `s` is 1 to 255 bytes of ASCII letters, digits, `-` and `.`, does not start or
  end with `.`, and has no `..`. Apple's rule for `CFBundleIdentifier` allows only those characters.
- `reach(records, id)`: `HostOnly(host)` only when the main-thread record's `host` passes `valid_bundle_id`; any other
  non-null `host` gives `None` (it can't name an app to bring forward).
- `Detail` gains `pub app: Option<String>`, serialized as `"app"`: the `localizedName` of the first running
  application (`NSRunningApplication`, not terminated) whose bundle id is the `HostOnly` host, else `null`; always
  `null` for `Inbox` and `None`. `session::detail` stays pure and sets `null`; the `session_detail` command fills it
  with `host::app_name`.
- `host::app_name(bundle_id)`: that `localizedName`, or `None` when no such app runs or it has no name.
- `host::open(bundle_id)`: `NotRunning` when no application with that bundle id is running (terminated ones don't
  count): nothing is launched. Otherwise it asks the first one to come forward and returns `Opened(name)`, `name`
  its `localizedName` or the bundle id; `Err` with the reason when the system refuses the request.
- How it asks: `activateWithOptions` with no options, on the calling (blocking) thread; `NSRunningApplication` is
  thread safe. Measured on macOS 27.0.1 from a background-launched accessory process with a non-activating panel
  shown, never active itself, with Finder frontmost and no hardware input for over 160 s: TextEdit was frontmost
  21 ms after the call from a background thread (3 of 3 runs), 26 ms from the main thread; `NSWorkspace`
  `openApplicationAtURL:configuration:` with `activates` also worked (22 ms), and with no call TextEdit never came
  forward. `activateWithOptions` is used because it can't launch: `openApplicationAtURL` would relaunch an app that
  quit between the lookup and the call. Not measured: the call right after a click on the panel (the end-to-end
  check, `tests/open_host_test.sh`, covers it).

#### Bridge

- Command `open_host(id: String) -> Result<HostOpen, String>`, run off the main thread like `prompt`: `Err` when `id`
  fails `valid_id`, or when `reach` against the last refresh's records is not `HostOnly` (`no app to open for this
  session`); else `host::open(host)`. Under `$NOTCH_TRACE` it logs one line to stderr: `host: opened <bundle id>`,
  `host: not running <bundle id>`, or `host: failed: <reason>`.
- It never calls `focus` and never makes the panel key.

#### Page

- For a card whose `reach.kind` is `host_only`, `#pbox` is hidden and `<button type="button" class="openapp"
  id="openApp">` takes its place: 388 x 34 at (16, 316) of `#content`, radius 10, background `#1c1c1c` (`#262626`
  while hovered), no border, text `Open <app>` centered in 600 12.5/34 px `#f5f5f5`. `<app>` is the detail's `app`
  when it is a non-empty string, else `reach.host`. For any other `reach` the button is hidden and the box shows as
  before. A null detail reply (`Session ended`) hides the button too.
- The hint line below it reads `It has no inbox, so notch-hud can only bring <app> forward.` in muted, replacing
  `It has no inbox notch-hud can send to. Reply in its own app.`.
- A press on the button keeps DOM focus where it is and starts no typing; the page never calls `focus` for it. A
  click calls `invoke('open_host', { id })` with the card's id, unless a call for this card is still in flight. The
  reply sets the hint:

  | Reply | Hint |
  |---|---|
  | `{kind: "opened", name}` | `Brought <name> forward.` in `#f5f5f5` |
  | `{kind: "not_running"}` | `<app> isn't running.` in amber |
  | rejected | `Couldn't bring <app> forward.` in amber |

  A reply that arrives after the card closed or shows another session changes nothing. The result hint stays while
  the card shows the same session with `reach` `host_only` and the same host; closing the card, showing another
  session, or a redraw with another `reach` or host clears it.
