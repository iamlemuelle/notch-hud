# notch-hud: video, session detail and prompt plan

Scope (2026-10-22): three features on the existing panel. A video player in the notch, a detail view per session row,
and a prompt box that sends text to that Claude Code session.
Design reference: the [notch-hud Sessions and Video prototype](https://claude.ai/artifact/W6jgY4nGA9NwnY7xYCbAcQ), built
on the approved prototype that `CONTRACT.md` section 4 cites. It shows the detail card, every prompt state, the video
stage, and the native window frame for each state.
Decisions D1-D10 are settled as recommended (2026-10-22, section 5).
`[unverified]` = not confirmed in current docs or a spike. "Measured" = a throwaway spike run on this Mac (macOS 27,
Claude Code 2.1.308 CLI, Desktop's bundled 2.1.304), deleted afterwards; each is described well enough to rerun.
"Inferred" = read from code, not run.

## 1. Feasibility verdicts

| Feature | Verdict | Evidence |
|---|---|---|
| 1. Video player in the notch | **Possible with caveats** | `<video>` plays in a WKWebView inside a borderless non-activating `NSPanel` while the panel is not key and the app is inactive (measured: `currentTime` 1.92 → 5.08 s over 3 s, `key=false active=false`). It plays from a custom URL scheme that serves byte ranges (measured: `play()` resolved, `readyState` 4, about 200 small `Range:` requests for a 130 KB clip), which is how Tauri's asset protocol serves files (`tauri-2.12.4/src/protocol/asset.rs:99-128`). wry turns autoplay on by default (`wry-0.57.0/src/lib.rs:881`, `wkwebview/mod.rs:363-366`). Caveats: it needs the `protocol-asset` feature and `assetProtocol` config (a config change), the bigger window covers more of the menu bar while it is open, and **the browser video that Now Playing detects can't be played in the notch**: the adapter payload carries no media URL (measured keys: `album artist artworkData artworkMimeType bundleIdentifier contentItemIdentifier durationMicros elapsedTimeMicros playbackRate playing processIdentifier timestampEpochMicros title`) |
| 2. Clickable sessions | **Possible** | Clicks already reach the page without activating the app (`main.rs:165-166`, `:636`, `CONTRACT.md` section 3 "Clicks", `tests/controls_test.sh`). The record holds everything the view shows: status, `cwd`, `event`, `tool_name`, `ts` (`session.rs:7-19`). The blocker is only that `Row` carries no session id (`session.rs:45-50`), so the amendment is to `CONTRACT.md` section 2, not the hook |
| 3. Prompt box | **Possible with caveats** | Every Claude Code session binds a per-session inbox socket and exports its path to hooks as `CLAUDE_CODE_MESSAGING_SOCKET`. The docs name "when you want a script or hook to post into a session" as a use ([cross-session messaging](https://code.claude.com/docs/en/cross-session-messaging#the-sessions-inbox-socket), [env vars](https://code.claude.com/docs/en/env-vars)). Measured: this Desktop session's environment has `CLAUDE_CODE_MESSAGING_SOCKET=/tmp/cc-socks/97149.sock`, so Desktop sessions bind one too. An idle session starts a turn with the message, and a busy one reads it between tool calls (docs). Caveats: **the message line format is undocumented** (open docs issues [anthropics/claude-code#103048](https://github.com/anthropics/claude-code/issues/103048), [#103123](https://github.com/anthropics/claude-code/issues/103123); plain text is reported to be ignored); the message arrives framed as coming from another session, not from you, so it can't approve a permission prompt; a session that bypasses permissions holds it for approval; the record needs a new `inbox` field (a `CONTRACT.md` section 1 amendment, measured at no cost to the hook); the panel needs a key-capable subclass (measured below). Not a hack: it is the documented inbox for local scripts, with one undocumented line. Phase 0 settles that line before any code |

## 2. Current-state blockers

| Blocker | Evidence | Consequence |
|---|---|---|
| Rows have no identity | `Row { label, status, detail }` (`session.rs:45-50`, `CONTRACT.md:64`); `summarize` builds rows from labels only (`session.rs:179-194`); the page renders `row.label` (`dist/index.html:751-758`) | A click can't say which session it meant. Labels are not unique: `· <id>` gets added and dropped (`session.rs:185-190`) |
| The app keeps the summary, not the records | `LATEST: Mutex<Option<Summary>>` (`main.rs:80`); `refresh_sessions` drops `records` after `summarize` (`main.rs:358-377`) | Nothing to look up `cwd`, `event`, `ts` or a socket by id |
| The record can't route a prompt | Hook fields are fixed: `session_id agent_id cwd event notification_type tool_name ts` (`hooks/notch-hook.sh:18-19`, `CONTRACT.md:11-15`). No pid, tty, host app or socket | No way to reach the session that wrote it |
| The panel can't be key | `Borderless \| NonactivatingPanel` on a plain `NSPanel` (`main.rs:165-166`, `:175-181`). Measured: a plain borderless non-activating `NSPanel` asked to `makeKey` stays `key=false` (2 of 2 valid runs) | No keyboard input reaches the page |
| Hover-out collapses everything | `expanded \| hover out \| after 250 ms, collapse` (`CONTRACT.md:653`) | Moving the pointer away to read, or watching video without hovering, closes the panel |
| One expanded size, tied to rows | `Layout { stage, pill, peek, banner, open, ear }` (`layout.rs:10-18`); `open` height is `196 + 18 + 4 + 22 * shown_rows + 14` (`layout.rs:47-50`, `CONTRACT.md:228`); `Shape`/`WindowMode` have no other open state (`layout.rs:21-36`) | No size for a detail card or a video stage |
| The open window is as wide as the stage | `WindowMode::Open` is `(stage.width, …)` (`layout.rs:80`, `CONTRACT.md:556`) and `stage` = `EXPANDED_WIDTH + 2 * MARGIN` (`layout.rs:55`) | Widening the stage for video would also widen the open window over the menu bar |
| One command family | `act` parses media actions only (`main.rs:545-572`, `CONTRACT.md` section 7 "Controls"); handlers registered at `main.rs:617` | New commands for detail, prompt, key focus and video |
| No local file access | `"security": { "csp": null }` and no `assetProtocol` (`tauri.conf.json:10`); tauri features lack `protocol-asset` (`Cargo.toml:10`) | The page can't load a local video |
| Drops never reach the page | Tauri's drag handler returns `true` (`tauri-runtime-wry-2.12.2/src/lib.rs:4786-4817`), so wry skips WebKit's own drop (`wry-0.57.0/src/wkwebview/drag_drop.rs:90-95`) | A dropped file arrives as paths in Tauri's `DragDrop` event, never as an HTML5 `File` |

Compatible already: `Record` has no `deny_unknown_fields` (`session.rs:7`), so records with new fields still parse in
today's build. The webview accepts first mouse (`main.rs:636`).

## 3. Design per feature

### 3.0 How text can reach a running session (#3)

| Route | How | Conflict with the live session | Verdict |
|---|---|---|---|
| `claude --resume <id> -p "…"` | A second process loads the transcript and runs a turn | Foreground session: the live process never sees that turn, and "if you resume the same session in two terminals without forking, messages from both interleave into one transcript" ([sessions](https://code.claude.com/docs/en/sessions#branch-a-session)). Background session: the prompt is handed to the running session, but not when stdio is piped or redirected; then it "sends nothing and exits with status 1" ([sessions](https://code.claude.com/docs/en/sessions#resume-a-running-background-session)). notch-hud spawns with pipes | Rejected: split history in the common case |
| Agent SDK `resume` | Same as above inside a library: `query({ resume })` continues the session from disk ([SDK sessions](https://code.claude.com/docs/en/agent-sdk/sessions#resume-by-id)). No attach-to-live API is documented | Same split history, plus a Node or Python runtime in the app | Rejected |
| Terminal input injection (`TIOCSTI`) | Push bytes into the session's tty input queue | Measured: from a process outside the tty's session, `TIOCSTI` fails `EACCES`; it works only for a process whose controlling tty it is. Needs root otherwise. Would also mix with whatever you are typing and does nothing for Desktop sessions | Not possible without root. Not planned |
| Terminal scripting (Terminal `do script`, iTerm2 `write text`, `tmux send-keys`) | Each terminal's own API | Needs the tab's tty (not in the record; hooks have no controlling terminal per [hooks](https://code.claude.com/docs/en/hooks)), Automation permission per terminal, and types into the TUI mid-turn. No Desktop equivalent | Not planned |
| Focus the host app and paste | Activate the app, then post ⌘V and Return | Needs Accessibility to post keys, overwrites your clipboard, races your typing, and can't pick a tab or a Desktop session | Rejected. **Focus only** (no paste) is the fallback in 3.3 |
| Channels | An MCP server declared `claude/channel` pushes `notifications/claude/channel` ([channels reference](https://code.claude.com/docs/en/channels-reference)) | Fully documented, arrives as a `<channel>` tag, but every session must be started with `--dangerously-load-development-channels server:<name>` plus a confirmation dialog (research preview, custom servers aren't on the allowlist), interactive only, and one server per session. A Desktop Code-tab session can't take CLI flags [unverified] | Possible, but it changes how you start every session. Not chosen |
| **Inbox socket** | Connect to the session's `CLAUDE_CODE_MESSAGING_SOCKET` and write one line | None to the transcript: the running session itself takes the message, queued mid-turn or as a new turn when idle (docs). It is framed as from another session: it can't answer a permission prompt or change settings. A receiving session in bypass mode holds it for approval; Desktop and `-p` sessions can't show that dialog and drop it after `dialogExpiry` (5 min) | **Chosen**, behind Phase 0 |

**Routing data.** Today's record (`session_id`, `cwd`) can't route. Proposed `CONTRACT.md` section 1 amendment, two
nullable fields copied from the hook's environment:

| Field | Source | Why | Status |
|---|---|---|---|
| `inbox` | `$CLAUDE_CODE_MESSAGING_SOCKET`, or `null` when unset or empty | The address for the prompt | Documented as exported to hooks (env-vars reference) |
| `host` | `$__CFBundleIdentifier`, or `null` | The app to bring forward in the fallback | Measured `com.anthropic.claudefordesktop` in a Desktop session's Bash environment. Present in hook environments and in terminal sessions [unverified] |

Hook cost, measured: the hook with an added `--arg inbox "${CLAUDE_CODE_MESSAGING_SOCKET:-}"` and the field, against
the original, 200 interleaved runs each on the same payload: median 14.3 ms vs 14.3 ms, p90 15.6 vs 15.1 ms. A second
`--arg` for `host` costs the same (inferred). The hook still prints nothing and exits 0: both values go through
`--arg`, so no input reaches the shell. A stale socket outlives its session (measured: `/tmp/cc-socks/35633.sock`
dated 2026-10-20 still present), and socket names are `<pid>.sock` (measured, undocumented), so after pid reuse two
records can name one socket. Routing refuses a socket that more than one live record names, unless it goes to the newest.

The token (`CLAUDE_CODE_MESSAGING_TOKEN`) is **not** recorded. It proves a sender is the session's own child, and
notch-hud is not one; sending it would bypass the bypass-mode hold on false grounds.

### 3.1 Clickable sessions (#2)

```
records (read_dir) ─▶ summarize ─▶ Summary{rows[{id,…}]} ─emit "sessions"─▶ page list
        │                                                                        │ click row
        └─▶ RECORDS snapshot (main.rs) ◀── invoke("session_detail", {id}) ───────┘
                    │ session::detail(records, id, now)
                    └─▶ Option<Detail> ─▶ page detail card (refreshed on every "sessions")
```

- **Rust (`CONTRACT.md` section 2 amendment).** `Row` gains `id: String` (the session id). New pure
  `session::detail(records: &[Record], id: &str, now: u64) -> Option<Detail>`, using the same grouping, superseding
  and decay as `summarize`:

  ```rust
  pub struct Detail {
      pub id: String, pub label: String, pub cwd: String, pub status: Status,
      pub event: String,             // newest kept record's event in the group
      pub event_ts: u64,
      pub tool_name: Option<String>, // newest kept record's tool
      pub agents: usize,             // live subagent records
      pub reach: Reach,              // section 3.3
  }
  ```

  `main.rs` keeps the last `records` beside `LATEST`. Command `session_detail(id)` validates `id` with the hook's
  rule (`[A-Za-z0-9_-]+`, anchored) and returns `Option<Detail>`.
- **Layout.** `Layout.detail: Size` = (420, `196 + 184`): the music block stays, and the session section is replaced
  by a header with a back control and the label (18), 4 pt, four 22 pt rows (status, folder, latest event with its
  tool and age, subagents), 10 pt, the prompt box (34), 6 pt, a one-line hint (14) for the delivery state, 10 pt.
  Look and states: the [prototype](https://claude.ai/artifact/W6jgY4nGA9NwnY7xYCbAcQ). New `Shape::Detail`. `WindowMode::Open` sizes to the larger of `open` and `detail`, so
  switching list ↔ detail never resizes the window; the clip springs between the two heights (the existing "layout
  change while expanded" rule, `CONTRACT.md` section 8).
- **Page.** `.srow` becomes a button with `data-id` (not the "N more" row). A click calls `session_detail` and, on
  `Some`, shows the card. Each `sessions` event refetches it. When the id is gone (`SessionEnd`), the card says
  `Session ended` and keeps the back control. Collapse resets to the list.
- **Focus.** None. Clicks already work in the non-key panel.

### 3.2 Key window for typing (#3)

Measured on a background-launched accessory bundle (`LSUIElement`, `open -g`) hosting a WKWebView with an `<input>`,
as `NSPanel` subclasses created as `Borderless | NonactivatingPanel`. Valid runs only; in 3 of 9 launches the spike
came up frontmost before any key request (activated by the launch, not by the calls under test), and those are
excluded.

| Step | Result |
|---|---|
| `canBecomeKey` gated by a flag, flag off, `orderFrontRegardless` | `key=false active=false`, frontmost app unchanged, video playing |
| Flag on, `makeKey()`, `makeFirstResponder(webview)`, focus the input | `key=true`, frontmost app unchanged; `NSApp.isActive` reads `true` |
| Key events dispatched in-process (`NSApp.sendEvent`) | The input's value is `hi notch` |
| Give back with `NSApp.deactivate()`, flag off | `key=false active=false` at once and 3 s later, frontmost unchanged |
| Give back with `orderOut` + `orderFrontRegardless`, flag off | Same |
| Give back with `resignKey()` | `key=false` at once, `key=true` again 3 s later. Apple says never to call it directly; don't |
| Plain borderless `NSPanel`, `makeKey()` | Stays `key=false` |

Not covered by the spike: real keystrokes from the keyboard (the Claude Code Bash tool lacks Accessibility, so it
can't post them; the desktop app's Terminal panel can), whether the previous app's window regains keyboard focus after the give-back,
and ⌘V/⌘A in the field with no visible menu bar [all unverified]. The Phase 3 check covers all three.

Design:

- `panel.rs` (new; moves `adopt_panel` out of `main.rs`, which is at 658 of 800 lines): `define_class!` a
  `NotchHudPanel: NSPanel` whose `canBecomeKeyWindow` returns a main-thread `KEY_WANTED: Cell<bool>`. Still created
  as non-activating at init: the click spike showed that only a panel created that way never activates the app
  (`CONTRACT.md` section 3 "Clicks").
- Command `focus(on: bool)`, synchronous like `surface` (`main.rs:584-591`), so it runs on the main thread in order.
  On: set the flag, `makeKeyWindow`, `makeFirstResponder(webview)`. Off: clear the flag, `NSApp.deactivate()`.
- An `NSWindowDidResignKeyNotification` observer on the panel clears the flag and emits `key` `false`, so a click in
  another app ends typing.
- Nothing else ever asks for key; the flag is off in every other state, so behavior outside the prompt box is today's.
- Page: a `typing` substate of `expanded` (detail card). While typing, hover-out doesn't collapse. Esc and `key:false`
  call `focus(false)` and return to the detail card. Return sends; Shift+Return adds a newline.
- Anything keyed on `NSApp.isActive` would misread while typing (measured `true` with the frontmost app unchanged).
  Nothing reads it today (`grep -rn isActive src-tauri/src`: no hits).

### 3.3 Prompt box (#3)

```
hook: inbox=$CLAUDE_CODE_MESSAGING_SOCKET ─▶ record ─▶ RECORDS
page: invoke("prompt", {id, text}) ─▶ prompt::parse ─▶ prompt::route(records, id) ─▶ Route::Inbox(path) | Route::Unreachable
                                                                │
                                         spawn_blocking: prompt::send(path, frame(text))
                                                                │ UnixStream, 2 s connect and write timeouts
                                                     Claude Code inbox ─▶ the session's next hook event ─▶ row status
```

- **`prompt.rs` (new, pure except `send`), `CONTRACT.md` new section 11 "Session prompt".**

  ```rust
  pub enum Reach { Inbox, HostOnly(String), None }       // in Detail; HostOnly carries the bundle id
  pub struct Prompt { pub id: String, pub text: String } // parse_prompt: id rule, 1..=8000 chars after trim, no NUL
  pub enum Route { Inbox(PathBuf) }
  pub fn route(records: &[Record], id: &str) -> Result<Route, String>; // main-thread record; absolute path; < 104 bytes; one live record per socket
  pub fn frame(text: &str) -> String;                    // one JSON line, Phase 0 decides the shape
  pub fn send(path: &Path, line: &str) -> Result<(), String>; // lstat: a socket, our uid, not a symlink
  ```

  `frame` will be `{"type":"user","message":{"role":"user","content":<text>}}\n` if Phase 0 confirms it; that shape
  comes from community clients and the repro in #93720, not from Anthropic [unverified].
- **Commands.** `prompt(id, text) -> Result<(), String>` off the main thread, like `act` (`main.rs:545-572`), with a
  `$NOTCH_TRACE` line `prompt: sent <id> <n> chars`, `prompt: rejected: <reason>` or `prompt: failed: <reason>`.
  `open_host(id)` for `Reach::HostOnly`: bring that app forward with `NSWorkspace` (cooperative activation from an
  inactive accessory app on macOS 27 [unverified]; the Phase 6 check decides).
- **Delivery feedback.** The socket gives no acknowledgement (documented for channels; for the inbox [unverified]).
  The page shows `Sent` when the write succeeds, then watches the row: a status change to `working` within 10 s
  confirms it; otherwise `No reply yet. It may be waiting for approval in that session.`.
- **Page.** The prompt box sits in the detail card. Disabled with a reason when `reach` is `None` (`This session
  can't take messages`), and replaced by `Open <app>` for `HostOnly`.

### 3.4 Video player (#1)

**Source for v1: a local file dropped on the panel.**

| Source | Verdict | Why |
|---|---|---|
| Local file (drop) | **v1** | Measured playback from a range-serving custom scheme. No network, no account. Tauri's `DragDrop` event already gives paths |
| Direct URL (mp4, HLS) | Later | Needs a paste target (the prompt box's key handling, Phase 3) and network error states. HLS in WKWebView on macOS [unverified] |
| YouTube embed | No | An `<iframe>` from a `tauri://` origin; YouTube embed referrer requirements and terms [unverified]. Fragile for a personal tool |
| PiP of the Now Playing browser video | No | No URL in the payload (measured keys above). Mirroring it would mean ScreenCaptureKit capture of the browser window (Screen Recording permission, captures the window, not the element) |

**Player: HTML5 `<video>` in the webview.** It sits inside `#panel`, so the `clip-path` spring, the radius and the
fades apply to it like any other layer (`CONTRACT.md` section 8: only `transform`, `opacity` and `clip-path`
animate). An `AVPlayerLayer` would be a native layer over the webview that CSS can't clip, so every frame of the
spring would need a matching native mask. WebKit uses AVFoundation under `<video>` for H.264/HEVC [unverified for
WKWebView specifically].

**Data flow.**

```
Finder drag ─▶ WryWebView (Tauri DragDrop: paths) ─▶ video::parse_path ─▶ asset_protocol_scope().allow_file(path)
           ─▶ VIDEO state, emit "video" {url: asset://localhost/…, name} ─▶ page: theater phase, <video src=url>
```

- **`video.rs` (new, pure), `CONTRACT.md` new section 12 "Video".** `parse_path(p) -> Result<Video, String>`:
  canonical absolute path, a regular file, extension in `mp4 m4v mov`, size > 0. One `Video { path, name }` at a
  time. The scope is widened per file at runtime; no static `$HOME/**` scope.
- **Config (in the Phase 8 PR, not now).** `protocol-asset` on `tauri` (`Cargo.toml:10`);
  `app.security.assetProtocol: { enable: true, scope: [] }` (`tauri.conf.json:10`).
- **Expanded size.** `THEATER_WIDTH` = 608. Video 576 × 324 (16:9, `object-fit: contain`) at (16,
  `notch_height + 8`), then 8 pt, a 36 pt control row (play/pause, seek as in section 7, close), then 12 pt. So
  `theater` = (608, `notch_height + 388`): 608 × 420 on a 32 pt notch, window 640 × 436. On a screen narrower than
  1216 pt the width clamps to half the screen width and the picture keeps 16:9, so the height shrinks with it (D8).
  `layout()` takes the screen width for this.
- **`layout.rs`.** `Layout` gains `detail` and `theater`; `Shape` gains `Detail` and `Theater`; `WindowMode` gains
  `Theater`. `stage.width` becomes `THEATER_WIDTH + 2 * MARGIN`, and `WindowMode::Open` becomes
  (`open.width + 2 * MARGIN`, …), so the open window keeps today's 452 pt width (amends `CONTRACT.md:553-556`). The
  `open_floor` rule stays `Open`-only; `Theater` has a fixed size.
- **Motion.** Same window rule as today: grow first (window `theater`, wait for `surface` to resolve), then the clip
  springs `open` → `theater` (366 ms), the music and session layers fade out in 80 ms, and the video layer fades in
  (140 ms, 80 ms delay). Close reverses it, and the window shrinks to `open` on the clip's `transitionend`, mirroring
  "shrink only after the shape is back" (`CONTRACT.md` section 8 "Window").
- **Pinned.** In `theater`, hover-out doesn't collapse; close or Esc (while key) leaves it. Close pauses and
  releases the `src`.
- **Now Playing loop.** WebKit may publish the panel's own video to Now Playing [unverified]; then the adapter would
  report notch-hud itself and fire a song banner. `media.rs` drops tracks whose `bundleIdentifier` is the app's own
  (`dev.lem.notchhud`, `tauri.conf.json:4`).

## 4. Phased build order

Each phase is one PR and ends with `cargo test`, the existing `tests/*.sh`, and `./check.sh` green, plus its own
check. Checks that post real input run in the desktop app's Terminal panel (Accessibility). No check launches or
kills the installed `com.lem.notch-hud`: like `tests/motion_test.sh` (`:25-27`, `:49`), each runs its own app process
with a temp `NOTCH_DIR` and kills only its own pids.

### Phase 0: inbox line spike (no product code; approved 2026-10-22, D1)

- **Outcome:** the exact line a non-child local process must write, and whether it's delivered.
- **Check:** a scratch script starts a throwaway `claude` session in an empty temp directory with
  `--settings <scratch file>`, whose only hook is a scratch copy of `hooks/notch-hook.sh` that adds the `inbox` field
  and writes to a temp `NOTCH_DIR`. The installed hook can't be used: it doesn't record `inbox` until Phase 4, and
  `~/.claude` stays untouched. The script reads the session's `inbox` from its record, writes the candidate line,
  and passes when a new record for that session (`UserPromptSubmit` or `PreToolUse`) appears within 30 s. Run once
  in default mode and once in bypass mode, then once against a Desktop session the user picks.
- **Exit:** if no line is delivered from a non-child process, #3 stops at the focus fallback (Phase 6), and Phases 4
  and 5 are dropped.

### Phase 1: session ids and the detail view (#2)

- **Outcome:** `Row.id`, `Detail`, `session::detail`, `session_detail`, `Layout.detail`, `Shape::Detail`, the card.
- **Checks:**
  - `cargo test`: `detail` against literal records (newest event wins, superseded subagents ignored, decay to
    `Waiting`, unknown id `None`); `Row.id` in `summarize` cases; `Layout.detail` literal sizes.
  - `tests/page_test.sh` extension: a `sessions` payload with ids, a click on row 2 calls `session_detail` with that
    id, the card shows the four fields, `sessions` without the id shows `Session ended`.
  - New `tests/detail_test.sh` (Terminal panel, like `controls_test.sh`): a real click on a seeded row puts `page:
    detail <id>` in `$NOTCH_TRACE`, and `NSWorkspace.frontmostApplication` is unchanged.

### Phase 2: key-capable panel, gate off

- **Outcome:** `panel.rs`, `NotchHudPanel`, the `focus` command, the resign-key observer. No UI calls `focus` yet.
- **Checks:** `controls_test.sh`, `motion_test.sh` and `check.sh` unchanged and green (same behavior with the flag
  off); `git diff --stat` shows `adopt_panel` moved, not rewritten.

### Phase 3: prompt box, typing only

- **Outcome:** the box in the card, the `typing` substate, Esc and `key:false` handling. Return logs
  `prompt: draft <n> chars` and sends nothing.
- **Check:** new `tests/prompt_test.sh` (Terminal panel): TextEdit frontmost with an empty document as a decoy; click
  a seeded row, click the box, post the keys `hello`, Return → trace `prompt: draft 5 chars`; Esc; post `xyz` → it
  lands in TextEdit (focus came back); `frontmostApplication` is TextEdit throughout. Also ⌘V of a test string set on a
  private pasteboard, so your clipboard is untouched.

### Phase 4: record amendment

- **Outcome:** `CONTRACT.md` section 1 gains `inbox` and `host`; the hook copies them through `--arg`.
- **Checks:** `tests/hook_test.sh` cases: with `CLAUDE_CODE_MESSAGING_SOCKET=/tmp/x.sock` the record has `"inbox":
  "/tmp/x.sock"`; unset or empty gives `null`; a value with quotes and newlines stays one JSON string; the hook stays
  silent with exit 0; the 100-run timing loop median ≤ 20 ms (PLAN.md section 7 budget). `cargo test`: records with
  and without the fields parse.

### Phase 5: send

- **Outcome:** `prompt.rs`, the `prompt` command, `Reach`, the feedback states.
- **Checks:** `cargo test`: `parse_prompt` rejects empty, over 8000 chars, NUL, a bad id; `route` rejects a relative
  path, a 104-byte path and a socket shared by two live records; `frame` against literal JSON lines (quotes, newlines,
  emoji). New `tests/inbox_test.sh` (Terminal panel): a fake inbox (Python `AF_UNIX` server in a temp dir, `-I`)
  named in a seeded record; type and Return; the server received exactly the expected line; trace `prompt: sent <id>
  5 chars`; a non-socket path gives `prompt: rejected`.

### Phase 6: focus fallback

- **Outcome:** `open_host` and `Open <app>` for `Reach::HostOnly`.
- **Check:** a seeded record with `host: "com.apple.TextEdit"` and no `inbox`; a click on `Open TextEdit` makes
  TextEdit frontmost within 2 s.

### Phase 7: theater layout (#1)

- **Outcome:** `Layout.theater`, `Shape::Theater`, `WindowMode::Theater`, the stage and open-window width change,
  the theater phase with an empty stage.
- **Checks:** `cargo test`: literal `theater`, `stage` and `window_size` values, and that the `Open` window stays
  452 pt wide. `tests/motion_test.sh` extension: trace `window: theater 640x436`, then `window: open` and `window:
  pill` after close and leave.

### Phase 8: local video

- **Outcome:** `video.rs`, the drop route, the runtime asset scope, the `video` event, `<video>` in the theater,
  the own-bundle Now Playing filter, and the config change.
- **Checks:** `cargo test`: `parse_path` against temp files (good mp4, directory, `.txt`, empty file, symlink
  canonicalized). New `tests/video_test.sh` (Terminal panel): `Drive.swift` gains a file drag; it drags a committed
  30 KB fixture (`tests/video/fixtures/clip.mp4`) onto the pill; trace `video: playing clip.mp4`, then two
  `video: t=` lines ≥ 1 s apart with a growing time; close → `window: open`. `scripts/perf.sh` while playing a 1080p
  file, to measure Tauri's protocol handler serving many small ranges.

## 5. Risks and decisions

**Risks**

| Risk | Evidence | Mitigation |
|---|---|---|
| The inbox line format changes or is never documented | #103048, #103123 open; plain text reported ignored | Phase 0 gate; `frame` in one function with literal tests; the 10 s "no reply yet" state makes a silent failure visible |
| Messages to bypass-mode or Desktop sessions are held, then dropped after 5 min | Cross-session messaging, "Control inbound messages" | Show the hint; never send the token |
| A prompt arrives as from another session, not from you | Same page, "How a session treats an incoming message" | Accepted (D2). Channels stays the documented alternative if the inbox line breaks |
| Stale socket or pid reuse | Measured stale socket from 2026-10-20; `<pid>.sock` names | `route` refuses a socket two live records share; connection refused → `This session isn't reachable` |
| Keyboard focus doesn't return to the previous app | Not covered by the spike | Phase 3 decoy check; if it fails, `focus(false)` also re-activates `host` |
| Physical keys or ⌘V don't reach the key panel | Spike used in-process events only | Phase 3 check before any send code |
| The theater window covers app menus on narrow screens | 640 pt centered on the notch | Theater is pinned only while open; collapse restores the pill frame |
| Range traffic through the asset protocol is slow for large files | About 200 range requests for a 130 KB clip (measured) | Phase 8 `perf.sh` run |
| Unexplained pause | 1 of 9 spike runs read `paused: true` after a focus change (non-looping 4 s clip, older spike build) | Phase 8 check plays across a `focus` on/off cycle |

**Decisions** (settled 2026-10-22, each as recommended)

| # | Decision | Chosen | Rejected | Why |
|---|---|---|---|---|
| D1 | Run Phase 0 against a real Claude Code session | A throwaway session in a temp dir, default mode then bypass, plus one Desktop session the user picks | One of the user's working sessions; skipping it | Settles the undocumented line before any send code, without risking real work |
| D2 | Prompt transport | Inbox socket, with focus-only as the fallback | Channels (a dev flag on every launch); focus-only alone | The only route that reaches a running Desktop or terminal session with no change to how sessions start |
| D3 | Record amendment | `inbox` + `host` | `inbox` only; none | One `--arg` each, measured at no cost; `host` feeds the fallback |
| D4 | Sessions in bypass mode | Show the "held for approval" hint | Sending the token to skip the hold | The token claims to be the session's own child, which notch-hud isn't |
| D5 | Detail view on collapse | Reset to the list | Remember the open card | A stale card on the next hover hides new attention rows |
| D6 | Video source v1 | Local file by drop | URL; YouTube; browser PiP | Measured to play; the others need network states, fail on embedding, or have no URL |
| D7 | Video stage while the pointer leaves | Pinned until closed, with a close button | Collapse on hover-out | Watching shouldn't need a hovering pointer |
| D8 | Video stage size | Fixed 608 × (notch + 388), clamped to half the screen width | A share of the screen | Fixed numbers keep `layout.rs` tests literal; the clamp covers narrow screens |
| D9 | Asset scope | Per file at runtime | Static `~/Movies/**` | Grants only what the user dropped |
| D10 | Order | #2 → #3 → #1 | #1 first | #3 builds on #2's card, and Phase 0 can run while Phase 1 is built |
