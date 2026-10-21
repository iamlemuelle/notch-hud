# notch-hud

A MacBook notch panel that shows what each Claude Code session is doing. The collapsed pill sits over the notch, above
the menu bar. Hover over it to see one row per session.

| Dot | Meaning |
|---|---|
| Amber, with `⚠ N` | N sessions wait on a permission prompt or on your answer |
| Green | At least one session is working |
| Gray | Every session is done or waiting for your next prompt |

Expanded rows read like `bianki — needs permission: Bash`, `lstack — working`, `patunay — done`.

## How it works

1. `hooks/notch-hook.sh` runs as a Claude Code hook on 12 events. It writes each session's latest event to
   `~/.claude/notch/sessions/<session_id>.json`, and each subagent's to `<session_id>.<agent_id>.json`.
2. The app (Tauri v2, Rust, one HTML page) reads that directory every second, works out a status per session, and
   draws the panel.

`CONTRACT.md` is the spec: the record format, the hook's rules, the status table, and the installer's behavior. Code
and tests are both written against it.

The hook runs synchronously on every tool call, so it has to stay small. It prints nothing, always exits 0, and took
14 ms median per call on an M4 MacBook. Exit code 2 would block a tool call, and printed output on some events
goes into Claude's context, which is why both are ruled out.

## Requirements

- macOS on a MacBook with a notch (tested on macOS 27.0.1). Without a notched display the app prints
  `notch-hud: no notched display` and exits.
- Rust (`brew install rustup`), and `jq` at `/usr/bin/jq`, which macOS ships.
- Claude Code.

## Install

```sh
cd src-tauri && cargo build --release && cd ..
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
./check.sh                            # end to end on the real screen, needs Screen Recording permission
```

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
