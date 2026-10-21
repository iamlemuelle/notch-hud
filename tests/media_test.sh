#!/bin/bash
# End to end: the real app reads a fixture player through the real adapter, notices when it stops, and falls back to
# AppleScript when the adapter is missing. Asserts on the app's NOTCH_TRACE lines (CONTRACT.md section 7).
set -u
root="$(cd "$(dirname "$0")/.." && pwd)"
export PATH="/opt/homebrew/opt/rustup/bin:$PATH"
fail=0
pass() { echo "PASS $1"; }
flunk() { echo "FAIL $1"; fail=1; }

"$root/scripts/build-adapter.sh" >/dev/null || { echo "FAIL adapter build"; exit 2; }
fixture_app="$("$root/tests/media/build-fixture.sh")" || { echo "FAIL fixture build"; exit 2; }
fixture="$fixture_app/Contents/MacOS/NowPlayingFixture"
(cd "$root/src-tauri" && cargo build --release -q) || { echo "FAIL app build"; exit 2; }

tmp="$(mktemp -d)"
export NOTCH_DIR="$tmp/sessions"
export NOTCH_TRACE=1
app=""; player=""
cleanup() { [ -n "$player" ] && kill "$player" 2>/dev/null; [ -n "$app" ] && kill "$app" 2>/dev/null; wait 2>/dev/null; }
trap cleanup EXIT

# Waits up to $2 seconds for a fixed string to appear in the log file $1.
wait_for() {
  local log="$1" secs="$2" want="$3" i=0
  while [ "$i" -lt $((secs * 5)) ]; do
    grep -qF -- "$want" "$log" && return 0
    sleep 0.2; i=$((i + 1))
  done
  return 1
}

# 1. Adapter path: a playing fixture shows up, then disappears when it quits.
log1="$tmp/app1.log"
"$root/src-tauri/target/release/notch-hud" 2>"$log1" & app=$!
"$fixture" "E2E Song" "E2E Artist" square 223 74 & player=$!
if wait_for "$log1" 10 'media: adapter ok playing "E2E Song" (dev.lem.notchhud.fixture)'; then pass "adapter reports the fixture"; else flunk "adapter reports the fixture"; fi
kill "$player"; wait "$player" 2>/dev/null; player=""
if wait_for "$log1" 10 'media: adapter ok nothing'; then pass "fixture stop clears the track"; else flunk "fixture stop clears the track"; fi
kill "$app"; wait "$app" 2>/dev/null; app=""

# 2. Isolation: no adapter, the app keeps running and falls back to AppleScript.
log2="$tmp/app2.log"
NOTCH_MEDIA_ADAPTER=/nonexistent "$root/src-tauri/target/release/notch-hud" 2>"$log2" & app=$!
if wait_for "$log2" 10 'media: adapter Failed ('; then pass "missing adapter is logged"; else flunk "missing adapter is logged"; fi
if wait_for "$log2" 10 'media: applescript '; then pass "fallback reports a state"; else flunk "fallback reports a state"; fi
if kill -0 "$app" 2>/dev/null; then pass "app still running"; else flunk "app still running"; fi

if [ "$fail" -ne 0 ]; then
  echo "--- app1.log"; cat "$log1"; echo "--- app2.log"; cat "$log2"
fi
exit "$fail"
