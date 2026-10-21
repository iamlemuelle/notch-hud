# notch-hud

A MacBook notch panel that shows what each Claude Code session is doing. The collapsed pill sits over the notch, above
the menu bar. Hover over it to see one row per session.

| Dot | Meaning |
|---|---|
| Amber, with `⚠ N` | N sessions wait on a permission prompt or on your answer |
| Green | At least one session is working |
| Gray | Every session is done or waiting for your next prompt |

Expanded rows read like `bianki — needs permission: Bash`, `lstack — working`, `patunay — done`.

It also shows what is playing: the right ear carries the artwork with a small equalizer (or a pause mark), and the
expanded panel adds a card with the artwork, title, artist, the app, a seek bar, and previous, play/pause and next
buttons. Anything macOS lists as Now Playing shows up, so Apple Music, Spotify, and video in Safari or Chrome (YouTube
included, shown as the browser) all work. A wide thumbnail switches the card to a video layout. Clicking a control
doesn't bring notch-hud to the front: the app you were in stays active.

## How it works

1. `hooks/notch-hook.sh` runs as a Claude Code hook on 12 events. It writes each session's latest event to
   `~/.claude/notch/sessions/<session_id>.json`, and each subagent's to `<session_id>.<agent_id>.json`.
2. The app (Tauri v2, Rust, one HTML page) reads that directory every second, works out a status per session, and
   draws the panel.

3. Now Playing comes from the vendored [mediaremote-adapter](vendor/VENDORED.md), run as a `/usr/bin/perl`
   subprocess that streams JSON. It relies on a private macOS framework through a loophole Apple could close, so when
   its self-test fails the app falls back to asking Music and Spotify directly over AppleScript (macOS asks once for
   Automation access) and keeps retrying the adapter every minute. The controls go back the same way: the adapter's
   `send` and `seek` commands, or `tell application "Music"` (or `"Spotify"`) when on the fallback.
4. The panel is a native non-activating `NSPanel` that hosts the Tauri webview. A plain Tauri window, even one turned
   into a panel after it is created, activates the app when clicked on macOS 27, which would take focus from the app
   you are working in.

`CONTRACT.md` is the spec: the record format, the hook's rules, the status table, and the installer's behavior. Code
and tests are both written against it.

The hook runs synchronously on every tool call, so it has to stay small. It prints nothing, always exits 0, and took
14 ms median per call on an M4 MacBook. Exit code 2 would block a tool call, and printed output on some events
goes into Claude's context, which is why both are ruled out.

## Requirements

- macOS on a MacBook with a notch (tested on macOS 27.0.1). Without a notched display the app prints
  `notch-hud: no notched display` and exits.
- Rust (`brew install rustup`), and `jq` at `/usr/bin/jq`, which macOS ships.
- `cmake` (`brew install cmake`) to build the Now Playing adapter.
- Claude Code.

## Install

```sh
cd src-tauri && cargo build --release && cd ..
./scripts/build-adapter.sh         # builds the Now Playing adapter into src-tauri/target/adapter (needs cmake)
./scripts/install-hooks.sh        # registers the hook in ~/.claude/settings.json, after a backup
./src-tauri/target/release/notch-hud &
```

`install-hooks.sh` writes a uniquely named backup next to `settings.json` first. It keeps every other hook, keeps the
file's permissions, follows a symlink, and running it twice registers each event once. `scripts/uninstall-hooks.sh`
removes exactly what it added.

To start the panel at login, run `./scripts/install-login.sh` after building. It writes
`~/Library/LaunchAgents/com.lem.notch-hud.plist` and loads it now. launchd restarts the app after a crash but not after
a clean exit, so a Mac without a notch doesn't relaunch it in a loop. Errors go to `~/Library/Logs/notch-hud.log`. After
a rebuild, run `launchctl kickstart -k gui/$(id -u)/com.lem.notch-hud` to load the new binary.
`./scripts/uninstall-login.sh` stops it and removes the plist.

## Tests

```sh
cd src-tauri && cargo test && cd ..   # status rules: grouping, priority, decay, pruning, contract values
./tests/hook_test.sh                  # the hook against real payloads in a temp directory
./tests/install_test.sh               # the installer against temp copies of settings.json
./tests/media_test.sh                 # Now Playing end to end: a fixture player through the real adapter, then the fallback
./tests/controls_test.sh              # real clicks on the controls drive the fixture player; the front app must not change
./check.sh                            # end to end on the real screen, needs Screen Recording permission
```

`controls_test.sh` posts mouse events, so the terminal running it needs Accessibility, and you must leave the mouse
alone for the ~20 s it runs: real input moves the pointer out from under it. It exits 2 (skipped) when it can't post
events or when a click would land on another window, such as an older notch-hud ordered above the test's panel.

`check.sh` builds and launches the app with sessions seeded through the real hook into a temp directory. It then
checks the window server (window level 27, above the menu bar's 24), moves the pointer to test hover, and samples
screen pixels: the amber dot, the expanded rows, and the dot turning green after a `Stop`. It never touches
`~/.claude`.

## Known limits

- Claude Code sends no event when you approve a permission prompt. After you approve a long-running tool, the session
  stays amber until that tool finishes.
- A manual "No" or Esc on a permission prompt also sends no event. The main thread's next prompt, `Stop`, or idle
  notification clears it.
- An interrupted turn sends no `Stop`. A session that has been "working" with no event for 10 minutes shows as waiting.
- Session files older than 3 hours are deleted.
- With the AppleScript fallback, controls work only for Music and Spotify, and only while that app is running (a click
  never launches it). Its state refreshes on the next 2 s poll.
- While nothing is playing, the adapter's self-test briefly
  publishes a test track at startup and after a restart, which other Now Playing displays can see for a moment.
- The adapter is looked up next to `target/<profile>/notch-hud`; a bundled `.app` will need a new path.
