# notch-hud

A MacBook notch panel that shows what each Claude Code session is doing. The collapsed pill sits over the notch, above
the menu bar. Hover over it to open the panel: what is playing on top, your sessions below.

The left ear holds a small avatar and the session count. The avatar acts out what your sessions are doing:

| Avatar | When |
|---|---|
| Surprised, count amber | A session waits on a permission prompt or on your answer |
| Sad, count amber | A session stopped with an error |
| Celebrating, for 4 s | A session just finished (with its banner) |
| Working | At least one session is working |
| Listening | Music is playing and nothing is working |
| Idle, then drowsy after 2 min, asleep after 5 min | Everything is quiet |

It ships with a default character. To use your own, make one in the
[Bible Strong avatar lab](https://github.com/smontlouis/bible-strong-avatar-lab), export it (`*.avatar.json`, schema
`bible-strong/avatar-definition` v1) and save it as `~/Library/Application Support/notch-hud/avatar.json`, or point
`NOTCH_AVATAR` at it. Check a file with `src-tauri/target/notch-hud.app/Contents/MacOS/notch-hud --check-avatar <file>`,
then restart the app (`launchctl kickstart -k gui/$(id -u)/com.lem.notch-hud`). The avatar plays the animations named
`surprised`, `sad`, `celebrate`, `working`, `listening`, `idle`, `drowsy` and `sleeping` (with fallbacks, CONTRACT.md
section 10); a missing one holds the `neutral` face. A file that is missing or invalid falls back to the default, and
the reason goes to `~/Library/Logs/notch-hud.log` (`avatar: ...`). Avatar files are personal: `.gitignore` keeps
`*.avatar.json` out of the repository.

Under a `Claude Code` header (`1 needs you`, `2 working` or `All quiet`), each session gets a row with its own dot,
like `bianki` with `needs permission: Bash` in amber, `lstack` `working`, `patunay` `done`. Up to four rows show,
then `N more`.

It also shows what is playing: the right ear carries the artwork with a small equalizer tinted by the artwork (or a
pause mark), and the expanded panel opens on a card with the artwork and a soft glow of its color, title, artist, the
app, a seek bar, and previous, play/pause and next buttons. With nothing playing, the card says so. Anything macOS lists as Now Playing shows up, so Apple Music, Spotify, and video in Safari or Chrome (YouTube
included, shown as the browser) all work. A wide thumbnail switches the card to a video layout. Clicking a control
doesn't bring notch-hud to the front: the app you were in stays active.

## How it works

1. `hooks/notch-hook.sh` runs as a Claude Code hook on 12 events. It writes each session's latest event to
   `~/.claude/notch/sessions/<session_id>.json`, and each subagent's to `<session_id>.<agent_id>.json`.
2. The app (Tauri v2, Rust, one HTML page) reads that directory every second, works out a status per session, and
   draws the panel.

3. The avatar is drawn by the vendored [`@bible-strong/avatar-core`](vendor/VENDORED.md) renderer. Rust validates
   the avatar file and picks the mood; the page paints only while the face moves, so a sleeping avatar costs nothing.

4. Now Playing comes from the vendored [mediaremote-adapter](vendor/VENDORED.md), run as a `/usr/bin/perl`
   subprocess that streams JSON. It relies on a private macOS framework through a loophole Apple could close, so when
   its self-test fails the app falls back to asking Music and Spotify directly over AppleScript (macOS asks once for
   Automation access) and keeps retrying the adapter every minute. The controls go back the same way: the adapter's
   `send` and `seek` commands, or `tell application "Music"` (or `"Spotify"`) when on the fallback.
5. The panel is a native non-activating `NSPanel` that hosts the Tauri webview. A plain Tauri window, even one turned
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
- The Tauri CLI, which builds the `.app`: `cargo install tauri-cli --version 2.12.1 --locked`.
- Claude Code.

## Install

The app runs as a signed bundle, `src-tauri/target/notch-hud.app`, so macOS keeps its permission grants (Automation
for Music and Spotify) across rebuilds. That needs a self-signed code signing certificate, made once:

1. Open Keychain Access (Spotlight: "Keychain Access").
2. In the menu bar choose **Keychain Access > Certificate Assistant > Create a Certificate…**
3. Set **Name** to `notch-hud Code Signing`, **Identity Type** to **Self Signed Root**, and **Certificate Type** to
   **Code Signing**. Click **Create**, then **Continue** at the warning about self-signed certificates, then **Done**.
4. Check that `security find-identity -p codesigning` lists `"notch-hud Code Signing"`.

The first build asks whether `codesign` may use the key; click **Always Allow**. To use a certificate with another
name, set `NOTCH_HUD_SIGN_IDENTITY` to that name. `NOTCH_HUD_SIGN_IDENTITY=-` signs ad hoc, for a machine without the
certificate, but then macOS treats every build as a new app and asks for permissions again.

```sh
./scripts/build-app.sh            # builds the adapter and the app, signs it: src-tauri/target/notch-hud.app
./scripts/install-hooks.sh        # registers the hook in ~/.claude/settings.json, after a backup
./scripts/install-login.sh        # starts the app now and at every login
```

`build-app.sh` checks for the certificate before it builds anything. It signs a copy and swaps it in with a rename, so
the login item never starts a half-built bundle. `codesign -d -r- src-tauri/target/notch-hud.app` prints the
requirement macOS matches grants against; it stays the same from one build to the next.

`install-hooks.sh` writes a uniquely named backup next to `settings.json` first. It keeps every other hook, keeps the
file's permissions, follows a symlink, and running it twice registers each event once. `scripts/uninstall-hooks.sh`
removes exactly what it added.

`install-login.sh` writes `~/Library/LaunchAgents/com.lem.notch-hud.plist`, pointing at
`src-tauri/target/notch-hud.app/Contents/MacOS/notch-hud` (`NOTCH_HUD_BIN` overrides it), and loads it now. launchd
restarts the app after a crash but not after a clean exit, so a Mac without a notch doesn't relaunch it in a loop.
Errors go to `~/Library/Logs/notch-hud.log`. After a rebuild, run
`launchctl kickstart -k gui/$(id -u)/com.lem.notch-hud` to load the new bundle; the build never restarts the app
itself. `./scripts/uninstall-login.sh` stops it and removes the plist.

The first time the adapter is not usable and the app falls back to AppleScript, macOS asks whether notch-hud may
control Music (or Spotify). The answer sticks to the certificate, not to one build.

For development, `cd src-tauri && cargo build --release` plus `./scripts/build-adapter.sh` still gives a plain binary
at `src-tauri/target/release/notch-hud`, which finds the adapter in `src-tauri/target/adapter`.

## Tests

```sh
cd src-tauri && cargo test && cd ..   # status rules: grouping, priority, decay, pruning, contract values
./tests/repo_test.sh                  # git-only rules: no avatar file committed, .gitignore, the vendored renderer's hash
./tests/hook_test.sh                  # the hook against real payloads in a temp directory
./tests/install_test.sh               # the installer against temp copies of settings.json
./tests/login_test.sh                 # the login item scripts against a fake launchctl
./tests/page_test.sh                  # the panel page in headless Chromium: motion timings, art move, banners, window calls
./tests/bundle_test.sh                # builds the signed app twice: Info.plist keys, adapter inside, same designated requirement
./tests/media_test.sh                 # Now Playing end to end on the bundled app: a fixture player through the real adapter, then the fallback
./tests/controls_test.sh              # real clicks on the controls drive the fixture player; the front app must not change
./tests/motion_test.sh                # the window grows on hover and for banners, and shrinks back to the pill
./tests/hover_key_test.sh             # typing starts and ends 12 times with the pointer on a card; hover never goes out
./tests/avatar_test.sh                # --check-avatar, the avatar: log lines and mood traces on the real app, and the .gitignore rule
./tests/avatar_schema_test.sh         # Rust's avatar parser against avatar-core 0.1.0 on a corpus of ~900 files (needs npm and the network)
./check.sh                            # end to end on the real screen, needs Screen Recording permission
```

CI (`.github/workflows/ci.yml`) runs cargo, repo, page, hook, installer and login tests on macOS 15 for every pull request and every push to `main`.
`page_test.sh` needs `playwright-core` and its headless Chromium shell; locally it uses the npx cache, or the paths in
`PLAYWRIGHT_CORE` and `CHROMIUM`, and exits 2 (skipped) without them. `media_test.sh`, `controls_test.sh`,
`motion_test.sh` and `check.sh` run only locally, because they launch the app, which exits without a notched display.
`bundle_test.sh` and `media_test.sh` build the signed bundle, so they need the certificate from Install (or
`NOTCH_HUD_SIGN_IDENTITY=-`, which skips the signature checks).
`check.sh` also needs Screen Recording permission.

`motion_test.sh` moves the real cursor (no clicks) onto the pill and away, and restores it at the end, so leave the
mouse alone for the ~30 s it runs.

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

## License

AGPL-3.0 (`LICENSE`). The avatar renderer, `@bible-strong/avatar-core`, is AGPL-3.0-only, so the whole app is. The
vendored mediaremote-adapter keeps its BSD-3-Clause license (`vendor/VENDORED.md`).
