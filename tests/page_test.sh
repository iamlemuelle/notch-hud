#!/bin/bash
# Page checks for CONTRACT.md section 8 (panel motion, banners, the grow-then-shrink window, the video stage) and section 10 "Page" (the
# avatar), section 4 "Detail card" (the session detail) and section 11 (the prompt box: typing, sending and its delivery states; the Open <app> button for a session with no inbox): dist/index.html runs in
# headless Chromium against a fake Tauri bridge. It moves no cursor and needs no display.
#
# Exit 0 when every check passes, 1 when one fails, 2 when playwright-core or the headless shell is missing.
# PLAYWRIGHT_CORE and CHROMIUM override the cached copies; NOTCH_PAGE overrides the page under test.
# PAGE_CPU_THROTTLE=8 slows each page's CPU eightfold, to reproduce a loaded CI runner on a fast machine.
set -u
root="$(cd "$(dirname "$0")/.." && pwd)"

export PLAYWRIGHT_CORE="${PLAYWRIGHT_CORE:-/Users/lem/.npm/_npx/a1722018f28f9df2/node_modules/playwright-core}"
export CHROMIUM="${CHROMIUM:-/Users/lem/Library/Caches/ms-playwright/chromium_headless_shell-1208/chrome-headless-shell-mac-arm64/chrome-headless-shell}"

command -v node >/dev/null || { echo "SKIP node is not installed"; exit 2; }
[ -d "$PLAYWRIGHT_CORE" ] || { echo "SKIP playwright-core not found at $PLAYWRIGHT_CORE (set PLAYWRIGHT_CORE)"; exit 2; }
[ -x "$CHROMIUM" ] || { echo "SKIP headless chromium not found at $CHROMIUM (set CHROMIUM)"; exit 2; }
[ -f "${NOTCH_PAGE:-$root/dist/index.html}" ] || { echo "FAIL no page at ${NOTCH_PAGE:-$root/dist/index.html}"; exit 1; }

status=0
node "$root/tests/page/motion-check.mjs"
rc=$?
[ "$rc" -eq 1 ] && status=1
[ "$rc" -eq 2 ] && [ "$status" -eq 0 ] && status=2
node "$root/tests/page/avatar-check.mjs"
rc=$?
[ "$rc" -eq 1 ] && status=1
[ "$rc" -eq 2 ] && [ "$status" -eq 0 ] && status=2
node "$root/tests/page/detail-check.mjs"
rc=$?
[ "$rc" -eq 1 ] && status=1
[ "$rc" -eq 2 ] && [ "$status" -eq 0 ] && status=2
node "$root/tests/page/prompt-check.mjs"
rc=$?
[ "$rc" -eq 1 ] && status=1
[ "$rc" -eq 2 ] && [ "$status" -eq 0 ] && status=2
node "$root/tests/page/open-host-check.mjs"
rc=$?
[ "$rc" -eq 1 ] && status=1
[ "$rc" -eq 2 ] && [ "$status" -eq 0 ] && status=2
node "$root/tests/page/theater-check.mjs"
rc=$?
[ "$rc" -eq 1 ] && status=1
[ "$rc" -eq 2 ] && [ "$status" -eq 0 ] && status=2
exit "$status"
