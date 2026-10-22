# notch-hud: provider, widget and pet plan

Scope (2026-10-21): personal build, clickable panel, first wave of Now Playing, Calendar, battery, timers, dev signals.
`[unverified]` = not confirmed in current docs. "Inferred" = read from code, not measured.

## 1. Current-state assessment

**Supports**

| Capability | Evidence |
|---|---|
| Pure, tested status logic | `src-tauri/src/session.rs:65-187`, spec `CONTRACT.md:81-115`, `session_tests.rs` |
| Bounded I/O that can't panic | `read_dir` capped at `MAX_FILES` (`session.rs:204-212`), `prune` (`session.rs:214-221`) |
| Panel above the menu bar on every Space | level 27 (`main.rs:14`, `:147`), collection behavior (`main.rs:148-153`) |
| Follows display changes | `place` (`main.rs:75-84`), called every poll (`main.rs:106-107`) |
| Silent, atomic hook | `hooks/notch-hook.sh:3`, temp file then `mv` (`:36-37`) |
| Real-screen proof | window-server and pixel checks (`check.swift:76-146`) |

**Blocks**

| Blocker | Evidence | Consequence |
|---|---|---|
| One hard-wired source | `poll_sessions` calls `prune`/`read_dir`/`summarize` (`main.rs:101-105`) | A new source needs a new thread and a new global |
| Session-shaped globals | `LATEST: Mutex<Option<Summary>>`, `EXPANDED_NOW` (`main.rs:58-59`) | Nowhere to keep other widgets' state |
| Session-only wire format | `emit("sessions")` (`main.rs:115`), `sessions` command (`:124-127`), `render(s)` (`dist/index.html:36-57`) | The page can render nothing else |
| Layout tied to rows | `44 + 22 * rows` (`main.rs:42-49`, `CONTRACT.md:143`) | No way to fit widgets of other heights |
| Polling | 1 s loop (`main.rs:165-168`), 50 ms hover loop (`:159-163`) | Up to 1 s latency. 2.1-3.2 idle wake-ups/s (measured 2026-10-22 with `scripts/perf.sh`; macOS coalesces the timers, so not the 21/s the loops suggest) |
| Instant resize | `setFrame_display` (`main.rs:82`), `display:none` (`dist/index.html:15-16`) | No animation possible |
| No click path | hover is computed natively because the webview gets no mouse events (`main.rs:86-99`) | Controls have no route in |
| Panic freezes the data | bare `thread::spawn` poll loop (`main.rs:165`) | A panic leaves stale rows with no sign of failure |
| Colors in three places | `dist/index.html:11-14`, `CONTRACT.md:147-148`, `check.swift:33-40` | A restyle silently breaks the checks |
| Not bundled | `"bundle": {"active": false}` (`tauri.conf.json:12`) | No Info.plist, so no Calendar or Apple Events permission prompts |

## 2. Target architecture

**Providers** push typed snapshots, the **hub** owns all state, **widgets** are pure renderers. This becomes
`CONTRACT.md` section 6; sections 1–5 stay unchanged.

```rust
pub enum WidgetState { Sessions(Summary), NowPlaying(Track), NextEvent(CalEvent), Power(Power), Timer(Timer), Dev(DevSignal) }
pub enum Health { Ok, Stale { since: u64 }, Failed { reason: String, retry_at: u64 }, Denied { permission: Permission } }
pub struct Slot { pub state: Option<WidgetState>, pub health: Health, pub updated: u64 }
pub struct Frame { pub seq: u64, pub slots: Vec<(ProviderId, Slot)>, pub layout: Layout, pub pet: PetState }

pub trait Provider: Send + 'static {
    fn id(&self) -> ProviderId;
    fn max_silence(&self) -> Duration;                          // past this, the hub marks the slot Stale
    fn run(self: Box<Self>, out: Sink, cmds: Receiver<Action>); // blocks on its own thread
}
```

```
 hooks/notch-hook.sh ─▶ ~/.claude/notch/sessions ─FSEvents─▶ SessionsProvider ┐
 mediaremote-adapter stream (stdout) ────────────────────────▶ MediaProvider ──┤ Update{id, Result<WidgetState>}
 EKEventStore change + boundary timer ───────────────────────▶ CalendarProvider┤ bounded sync_channel(64),
 IOPSNotificationCreateRunLoopSource ────────────────────────▶ PowerProvider ──┤ try_send, latest wins
 gh / git subprocess (60 s) ─────────────────────────────────▶ DevProvider ────┘
                                                                     │
                     ┌───────────────────────────────────────────────▼────────┐
                     │ Hub thread: slots, dedupe, watchdog, layout, arbiter,  │
                     │ pet FSM, restart with backoff                          │
                     └──────┬───────────────────────────────▲─────────────────┘
          Channel<Frame>    │ (ordered)                     │ invoke("act", {provider, action})
                     ┌──────▼───────────────────────────────┴─────────────────┐
                     │ Webview: widget renderers, CSS motion, pet sprite      │
                     └──────┬─────────────────────────────────────────────────┘
          main thread       │ hit rect, expanded flag
                     ┌──────▼─────────────────────────────────────────────────┐
                     │ AppKit: NSWindow frame, level 27, hover, click routing │
                     └────────────────────────────────────────────────────────┘
```

| Concern | Decision |
|---|---|
| Push vs poll | Sessions: an FSEvents watch (the `notify` crate's default macOS backend) with a 50 ms debounce, a timer for the next decay or TTL deadline (`session.rs:60-61`), and a 30 s safety rescan. Only CI polls (60 s) |
| Rust↔webview | One `tauri::ipc::Channel<Frame>`, which Tauri documents as fast and ordered, where events are neither. One `act` command, parsed into a closed `Action` enum at the boundary |
| State ownership | The hub is the only writer. The main thread owns the window and reads `Layout`. The page keeps no state of its own |
| Failure isolation | Each provider runs under `catch_unwind`. A panic or error sets `Failed` and restarts it with exponential backoff and jitter (1 s → 60 s). Past `max_silence` a slot goes `Stale` and dims. Subprocesses are respawned. Each widget renders in `try/catch` and falls back to a muted chip |
| Permissions | A refusal sets `Denied`, a normal state with no retry loop |

## 3. Integration matrix

Calendar and Apple Events need a bundled `.app` whose Info.plist carries the usage keys (Phase 4).

| Integration | Access mechanism | Permission | Stability risk | Shows / controls | Fallback |
|---|---|---|---|---|---|
| Claude Code sessions | The hook writes JSON (`CONTRACT.md` §1), watched with FSEvents | None | Public (documented hook events) | Status rows. Hide a row until its next event | Today's 1 s poll |
| Now Playing, system-wide | **PRIVATE** MediaRemote, via `mediaremote-adapter`: `/usr/bin/perl` loads a helper framework; `stream` and `send` | None. It works because `com.apple.*` bundle ids may use MediaRemote and perl reports `com.apple.perl` | **Private framework.** The adapter's README says direct loading has been "completely non-functional" since macOS 15.4 and that its API may break. Apple can close the loophole. macOS 27 [unverified] | Track, artwork, elapsed time. Play/pause, next, seek | Per-app row below, picked when the adapter's `test` exits non-zero |
| Now Playing, per app | Apple Events to Music and Spotify (`NSAppleScript`/ScriptingBridge) | `NSAppleEventsUsageDescription` (Apple: required), plus Automation consent per app | Public. Spotify's dictionary [unverified] | Track, play/pause, next | Hide the widget (`Denied`) |
| Calendar | EventKit `requestFullAccessToEvents` (macOS 14+). The `EKEventStoreChanged` notification [unverified] plus a timer | `NSCalendarsFullAccessUsageDescription` | Public | Next event and a countdown. Open in Calendar [unverified] | "Calendar off" chip |
| Battery | IOKit `IOPSCopyPowerSourcesInfo` + `IOPSNotificationCreateRunLoopSource` (macOS 10.2+, push) | None | Public | Percent, charging, time left | `pmset -g batt` (scraping) |
| Timers | In-process, in the hub | None | None | Countdown. Start, pause, cancel | n/a |
| CI | `gh run list --json`, every 60 s, for repos in session `cwd`s | Existing `gh` login | Public CLI, rate-limited | Last run status. Open the run | `Stale` after 3 failures |
| Git | `git status --porcelain=v2 --branch` on session events | None | Stable plumbing format | Branch, dirty count | Omit the field |
| Clicks | Tauri `set_ignore_cursor_events` (2.12.5) outside the hit rect. Inside it, `acceptsFirstMouse` or an `NSPanel` with `nonactivatingPanel` | None | Public AppKit, but Tauri makes an `NSWindow`. WKWebView first-mouse in a background accessory app [unverified] | Every control above | Native `mouseDown` hit-testing, like `main.rs:88-99` |

## 4. Brand and motion system

**Tokens.** `dist/tokens.css` is the only source of color, and `check.swift` reads its hex values. Geometry (ear 60 pt
`main.rs:15`, width 420 `main.rs:16`, row 22 pt) lives in Rust `layout.rs`, because Rust sizes the window, and
reaches the page in `Frame.layout`.

| Tier | Examples (Phase 1 keeps today's values) |
|---|---|
| Primitive | `--amber-500: #f5a623`, `--green-500: #34d058`, `--gray-500: #8b8b8b`, `--black: #000` |
| Semantic | `--status-attention`, `--status-working`, `--status-idle`, `--surface-pill`, `--text-muted` |
| Component | `--pill-radius: 14px`, `--dot-size: 8px`, `--glow: 0 0 6px` |
| Motion | `--dur-peek`, `--ease-out`, `--spring-expand` |

**Window strategy.** On hover intent Rust grows the window once and the page animates a clip on the black shape.
On collapse the page animates, then Rust shrinks the window on `transitionend`. Hit-testing uses the shape's rect
from `Layout`. At most two frame changes per interaction.

| Transition | Spec |
|---|---|
| Hover intent | 80 ms dwell, so a passing cursor doesn't trigger it |
| Peek (ears brighten, +4 pt) | 120 ms, `cubic-bezier(0.2, 0, 0, 1)` |
| Expand (clip) | Spring: stiffness 380, damping 32, mass 1. ζ = 32/(2·√380) ≈ 0.82, settling within 2% in about 250 ms. Shipped as a CSS `linear()` curve of about 40 points (Baseline since December 2023 per MDN) |
| Content in | 140 ms opacity and a 4 pt rise, after an 80 ms delay, `ease-out` |
| Collapse | Content out in 80 ms, then the clip in 200 ms `cubic-bezier(0.4, 0, 1, 1)`, no overshoot |
| Leave grace | 250 ms |
| Ear widget swap | 160 ms crossfade |
| Reduced motion | 120 ms fades only (WKWebView honoring the setting [unverified]) |

**60 fps.** Only `transform`, `opacity` and `clip-path` animate, and Rust never sets the frame once per animation
frame. `requestAnimationFrame` deltas go to `$NOTCH_TRACE`, a diagnostics log that is off by default. Target: p95 ≤
17.5 ms, and at most 2 frames over 33 ms per cycle.

**Space sharing.** Rust's layout solver assigns zones, and nothing draws over the notch itself.

| Zone | Owner |
|---|---|
| Left ear | Sessions dot and count, always (`CONTRACT.md:147`) |
| Right ear | One chip, by arbiter priority: attention > running timer > event < 10 min > now playing > low battery |
| Right ear, outer 20 pt | The pet perch, only when the chip fits in 40 pt. Otherwise the pet hides |
| Expanded | Stacked widget sections. The perch is a reserved 24×24 pt corner outside every hit rect |

## 5. Pet system

**Format.** A 2x PNG sprite atlas (48 px frames, 24 pt pet) plus a JSON manifest, played by CSS `steps()` on
`background-position`: compositor-only, no per-frame JS.

```json
{"id":"pet","size":[24,24],"anchor":[12,22],
 "animations":{"idle":{"row":0,"frames":6,"fps":6,"loop":true},"sleep":{"row":1,"frames":1},
               "cheer":{"row":2,"frames":8,"fps":12},"wince":{"row":3,"frames":6,"fps":12},
               "point":{"row":4,"frames":6,"fps":10},"interact":{"row":5,"frames":10,"fps":12}}}
```

**State machine.** A pure Rust `fn next(PetState, PetEvent, now) -> PetState`, owned by the hub. The page only plays
the clip that `Frame.pet` names.

| From | Event | To |
|---|---|---|
| idle | event with salience ≥ medium | react (one clip, then idle) |
| idle | no event for 5 min, or collapsed for 60 s with nothing needing attention | sleep |
| sleep | high-salience event | react |
| sleep | low or medium event | sleep |
| idle, sleep | click on the pet | interact (2 s) |
| react, interact | the clip ends | idle |
| any | no perch allocated | hidden (the state is kept, rendering stops) |

**Event bus.** The hub diffs consecutive `Frame`s into `PetEvent`s, so providers never know pets exist. At most one
react every 10 s; high salience bypasses the limit.

| PetEvent | Salience | Clip |
|---|---|---|
| `SessionNeedsYou` (→ NeedsPermission or NeedsInput) | high | point |
| `SessionDone` (Working → Done) | medium | cheer |
| `SessionError`, `CiFailed` | high | wince |
| `EventSoon` (≤ 5 min), `BatteryLow` (≤ 10%) | medium | point |
| `CiPassed`, `TrackChanged` | low | none |

**CPU budget.** Asleep is one static frame, adding 0%. Idle plays at 6 fps and adds ≤ 0.3% of one core over baseline.
Animation pauses while the screen is locked (NSWorkspace session notifications [unverified]).

## 6. Phased roadmap

Every phase ends with `cargo test`, both `tests/*.sh` and `./check.sh` green, plus:

### Phase 1: provider refactor, sessions as the first provider, behavior unchanged

- **Outcome:** `Provider`, `Hub`, `Slot`, `Health`. `SessionsProvider` wraps `session.rs` unedited, polls at 1 s,
  and keeps the `sessions` event and command. Identical pixels.
- **Exit checks:**
  - `git diff --quiet main -- src-tauri/src/session.rs src-tauri/src/session_tests.rs dist/index.html hooks/` exits 0.
  - `git diff -U0 main -- CONTRACT.md | grep -c '^-[^-]'` prints 0 (lines added, none removed).
  - New `cargo test` cases: a panicking provider gives `Failed` while another slot still emits. Silence past
    `max_silence` gives `Stale`. Identical updates emit once. Backoff caps at 60 s.
  - `./check.sh` criteria 1–6 pass unchanged (`check.swift:78-146`).
- **Risks:** the globals (`main.rs:58-59`) are tangled with `place`; `catch_unwind` needs `UnwindSafe` boundaries; a
  moved dedupe can shift emit timing.

### Phase 2: push pipeline and channel

- **Outcome:** FSEvents and deadline timers replace the poll. `Channel<Frame>` replaces the `sessions` event.
  `Layout` comes from Rust.
- **Exit checks:**
  - `check.swift` 7: after a `Stop` goes through the hook, the dot turns green within 300 ms (today the check allows
    3 s, `check.swift:138`).
  - `cargo test`: the next wake is the earlier of the decay flip and the TTL expiry, against literal timestamps.
  - New `scripts/perf.sh`: idle wake-ups ≤ 5/s.
- **Risks:** FSEvents coalescing latency; missed events (the rescan covers them); the new `notify` dependency.

### Phase 3: clickable panel, tokens, motion

- **Outcome:** click routing, the section 4 motion, `tokens.css`, and a spike on a tracking area in place of the
  50 ms hover poll [unverified].
- **Exit checks:**
  - `check.swift` 8: a synthesized click on a Done row hides it within 300 ms, and
    `NSWorkspace.frontmostApplication` is unchanged (the app didn't activate).
  - `check.swift` 9: once collapsed, the window bounds equal the pill again, so no transparent area is left over the
    menu bar.
  - `check.swift` 10: over 5 hover cycles, `$NOTCH_TRACE` shows p95 ≤ 17.5 ms.
  - Criteria 5–6 read their colors from `tokens.css`.
- **Risks:** WKWebView's first-mouse behavior; the oversized window swallowing menu-bar clicks.

### Phase 4: public-API providers, bundled app

- **Outcome:** Power, Timer and Calendar providers and the arbiter. Bundling on, with an Info.plist and a stable
  signing identity.
- **Exit checks:**
  - `plutil -extract NSCalendarsFullAccessUsageDescription raw <app>/Contents/Info.plist` prints a non-empty string.
  - `codesign -dv <app>` doesn't report `adhoc`.
  - `cargo test`: power mapping, `next_event(events, now)`, and the arbiter order, each against literal cases.
    `Denied` renders the chip.
- **Risks:** TCC grants may reset when the signature changes [unverified]; EventKit calls back on an arbitrary queue
  (Apple docs).

### Phase 5: Now Playing and dev signals

- **Outcome:** the adapter as the primary path, the AppleScript fallback, and the `gh` and `git` providers.
- **Exit checks:**
  - `cargo test`: the adapter's JSON lines parse from recorded fixtures, and a non-zero `test` exit selects the
    fallback.
  - `check.sh` with the adapter path set to `/nonexistent`: criteria 1–6 still pass and `$NOTCH_TRACE` logs
    `media: Failed`. That is isolation shown on the real screen.
- **Risks:** Apple closing the loophole; adapter API breaks; `gh` rate limits.

### Phase 6: pets

- **Outcome:** the sprite pipeline, the FSM, the event bus, and the perches.
- **Exit checks:**
  - `cargo test`: one case per FSM table row, plus the rate limit. Across all 32 on/off combinations of the five
    right-ear widgets, the pet rect never intersects a widget rect.
  - `scripts/perf.sh`: idle CPU with the pet asleep is within 0.1 percentage points of the Phase 5 baseline.
- **Risks:** the art takes time; WebKit may keep animating while occluded.

## 7. Performance and reliability budgets

`scripts/perf.sh` samples the release build for 60 s: the app pid plus WebKit `WebContent` processes that appear
after launch (`pgrep` diff, [unverified attribution]).

| Metric | Budget | Measured by |
|---|---|---|
| Idle CPU (collapsed, pet asleep) | ≤ 0.5% of one core, 60 s average | `top -l 61 -s 1 -stats pid,cpu` |
| CPU while expanding | ≤ 15% of one core, over 5 cycles | the same, while `check.swift` hovers |
| Memory | ≤ 150 MB combined footprint | `footprint` per pid [unverified flags] |
| Idle wake-ups | ≤ 5/s, target ≤ 1/s. Measured 2026-10-22, idle, side by side: 2.1/s on #8, 2.3/s once sessions are event-driven (within noise; `idlew` does not resolve the session thread, which `refresh:` traces show going from 1/s to 0.2/s) | `scripts/perf.sh` (`top -stats idlew`, app pid only, WebContent not counted) |
| Event-to-pixel latency | p95 ≤ 300 ms | `check.swift` 7 |
| Frame time | p95 ≤ 17.5 ms | `$NOTCH_TRACE` |
| Hook cost | median ≤ 20 ms (14 ms today, `README.md:24-25`) | a 100-run timing loop in `tests/hook_test.sh` |
| Provider failure | The panel keeps rendering, with 1 s → 60 s jittered restart | `cargo test`, `check.sh` (Phase 5) |
| Crash | Relaunch on crash (LaunchAgent, `README.md:47-48`) | `kill -SEGV`, then `pgrep` within 10 s |

## 8. Open decisions

| Decision | Options | Recommendation |
|---|---|---|
| Brand name | Keep `notch-hud`, or an invented word | Decide before Phase 4. TCC grants key on the bundle id `dev.lem.notchhud` (`tauri.conf.json:4`), so a later rename re-prompts every permission (inferred) |
| Aesthetic | Hardware-native (black, with color only for meaning) or expressive | Hardware-native. The pill has to read as part of the notch (`check.swift:99` asserts black), and scarce color keeps amber urgent |
| Pet concept | Creature, object, or abstract | An original abstract creature drawn from the pill's geometry: a 24 pt rounded form with two dot eyes, in two token colors. It reads at tiny sizes and resembles no existing character |
| Distribution | Ad-hoc, a stable self-signed identity, or notarized | Personal (decided), with a stable self-signed identity so grants survive rebuilds [unverified]. No notarization |
| Now Playing source | The adapter (private) or AppleScript only (public) | The adapter, with AppleScript as the fallback. Fine for a personal build |
| New dependencies | `notify`, a vendored `mediaremote-adapter`, `tauri-nspanel` | Approve the first two. Use the `objc2` crates already in `Cargo.toml:13-15` in place of `tauri-nspanel` |
| Hover mechanism | Keep the 50 ms poll, or a tracking area | Settle it with the Phase 3 spike's wake-up numbers |
