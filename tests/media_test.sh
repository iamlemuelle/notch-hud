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
adapter_dir="$root/src-tauri/target/adapter"
orphan_pat="$adapter_dir|MediaRemoteAdapterTestClient"
cleanup() {
  [ -n "$player" ] && kill "$player" 2>/dev/null
  [ -n "$app" ] && kill "$app" 2>/dev/null
  pkill -KILL -f "$adapter_dir" 2>/dev/null
  wait 2>/dev/null
}
trap cleanup EXIT

# Waits up to $2 seconds for a fixed string to appear in the log file $1, looking only at lines after line $4.
wait_for() {
  local log="$1" secs="$2" want="$3" after="${4:-0}" i=0
  while [ "$i" -lt $((secs * 5)) ]; do
    tail -n +$((after + 1)) "$log" | grep -qF -- "$want" && return 0
    sleep 0.2; i=$((i + 1))
  done
  return 1
}

# Prints the pid $1 and every descendant of it.
descendants() {
  local p
  echo "$1"
  for p in $(pgrep -P "$1" 2>/dev/null); do descendants "$p"; done
}

# Prints the pid of this test's adapter stream process, a descendant of app pid $1.
stream_pid() {
  local p
  for p in $(descendants "$1"); do
    ps -o command= -p "$p" 2>/dev/null | grep -qE "mediaremote-adapter\.pl.* stream( |$)" && { echo "$p"; return 0; }
  done
  return 1
}

lines() { wc -l <"$1" | tr -d ' '; }

# 1. Adapter path: a playing fixture shows up and disappears when it quits; a dead stream clears the track and
# recovers; killing the app leaves no adapter process behind.
log1="$tmp/app1.log"
"$root/src-tauri/target/release/notch-hud" 2>"$log1" & app=$!
"$fixture" "E2E Song" "E2E Artist" square 223 74 & player=$!
playing='media: adapter ok playing "E2E Song" (dev.lem.notchhud.fixture)'
if wait_for "$log1" 10 "$playing"; then pass "adapter reports the fixture"; else flunk "adapter reports the fixture"; fi

mark="$(lines "$log1")"
kill "$player"; wait "$player" 2>/dev/null; player=""
if wait_for "$log1" 10 'media: adapter ok nothing' "$mark"; then pass "fixture stop clears the track"; else flunk "fixture stop clears the track"; fi

mark="$(lines "$log1")"
"$fixture" "E2E Song" "E2E Artist" square 223 74 & player=$!
wait_for "$log1" 10 "$playing" "$mark" || echo "note: fixture did not show again before the stream kill"
spid="$(stream_pid "$app")"
mark="$(lines "$log1")"
if [ -n "$spid" ]; then kill -KILL "$spid" 2>/dev/null; else echo "note: no adapter stream process found under app $app"; fi
if [ -n "$spid" ] && wait_for "$log1" 10 'media: adapter failed nothing' "$mark"; then pass "stream death clears the track"; else flunk "stream death clears the track"; fi
if [ -n "$spid" ] && wait_for "$log1" 15 "$playing" "$mark"; then pass "stream restarts and shows the fixture again"; else flunk "stream restarts and shows the fixture again"; fi

kill -KILL "$app"; wait "$app" 2>/dev/null; app=""
i=0; left=""
while [ "$i" -lt 25 ]; do
  left="$(pgrep -fl -- "$orphan_pat")"
  [ -z "$left" ] && break
  sleep 0.2; i=$((i + 1))
done
if [ -z "$left" ]; then pass "no orphan adapter processes after app SIGKILL"; else
  flunk "no orphan adapter processes after app SIGKILL"
  echo "left behind:"; echo "$left"
  pkill -KILL -f -- "$orphan_pat" 2>/dev/null
fi
kill "$player" 2>/dev/null; wait "$player" 2>/dev/null; player=""

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
