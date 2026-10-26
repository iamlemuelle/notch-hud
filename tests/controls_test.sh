#!/bin/bash
# End to end, on the real panel: real mouse clicks on play/pause, next, previous and the seek bar change a fixture
# player's playback, and the frontmost app never changes (CONTRACT.md sections 3 "Clicks" and 7 "Controls").
#
# It posts mouse events, so the terminal running it needs Accessibility. Exit 0 when every check passes, 1 when one
# fails, 2 when the environment cannot run it. Do not move the mouse or switch apps while it runs.
set -u
unset CLAUDE_CODE_MESSAGING_SOCKET CLAUDE_CODE_MESSAGING_TOKEN  # never let a test reach a real session's inbox
root="$(cd "$(dirname "$0")/.." && pwd)"
export PATH="/opt/homebrew/opt/rustup/bin:$PATH"
fail=0
pass() { echo "PASS $1"; }
flunk() { echo "FAIL $1"; fail=1; }

tmp="$(mktemp -d)"
fixture_pid=""; app=""; watcher=""; cursor_x=""; cursor_y=""
drive="$tmp/drive"
cleanup() {
  [ -n "$cursor_x" ] && "$drive" warp "$cursor_x" "$cursor_y" 2>/dev/null
  [ -n "$watcher" ] && kill "$watcher" 2>/dev/null
  [ -n "$fixture_pid" ] && kill "$fixture_pid" 2>/dev/null
  [ -n "$app" ] && kill "$app" 2>/dev/null
  wait 2>/dev/null
}
trap cleanup EXIT

swiftc -O "$root/tests/controls/Drive.swift" -o "$drive" 2>"$tmp/swiftc.log" || { cat "$tmp/swiftc.log"; echo "FAIL driver build"; exit 2; }
"$drive" access || {
  echo "SKIP needs Accessibility to post events: run from a terminal that has it (the Claude app's Terminal panel)"
  exit 2
}

"$root/scripts/build-adapter.sh" >/dev/null || { echo "FAIL adapter build"; exit 2; }
fixture_app="$("$root/tests/media/build-fixture.sh")" || { echo "FAIL fixture build"; exit 2; }
fixture="$fixture_app/Contents/MacOS/NowPlayingFixture"
(cd "$root/src-tauri" && cargo build --release -q) || { echo "FAIL app build"; exit 2; }

export NOTCH_DIR="$tmp/sessions"
export NOTCH_TRACE=1
fxlog="$tmp/fixture.log"; applog="$tmp/app.log"; watchlog="$tmp/watch.log"
: >"$fxlog"

# Waits up to $2 seconds for a fixed string in log $1, looking only at lines after line $4.
wait_for() {
  local log="$1" secs="$2" want="$3" after="${4:-0}" i=0
  while [ "$i" -lt $((secs * 5)) ]; do
    tail -n +$((after + 1)) "$log" | grep -qF -- "$want" && return 0
    sleep 0.2; i=$((i + 1))
  done
  return 1
}
lines() { wc -l <"$1" | tr -d ' '; }

read -r cursor_x cursor_y < <("$drive" cursor)

"$fixture" "Ctl Song" "Ctl Artist" square 223 74 >"$fxlog" 2>&1 & fixture_pid=$!
"$root/src-tauri/target/release/notch-hud" 2>"$applog" & app=$!

if ! wait_for "$applog" 15 'page: loaded'; then flunk "page loads"; echo "--- app.log"; cat "$applog"; exit 1; fi
if ! wait_for "$applog" 15 'media: adapter ok playing "Ctl Song" (dev.lem.notchhud.fixture)'; then
  flunk "app sees the fixture"; echo "--- app.log"; cat "$applog"; exit 1
fi
pass "page loaded and the fixture is playing"

"$drive" watch >"$watchlog" 2>&1 & watcher=$!
wait_for "$watchlog" 5 watching || { echo "FAIL activation watcher did not start"; exit 2; }
front0="$("$drive" front)"

# The panel must be on top at the click points (the user's own notch-hud may be running at the same level). `drive hover`
# waits for the open window (452 wide, panel 420 wide inset by 16) and prints the panel's left edge as x0.
x0_line="$("$drive" hover "$app" 2>&1)"; rc=$?
if [ "$rc" -eq 2 ]; then echo "SKIP $x0_line"; exit 2; fi
if [ "$rc" -ne 0 ]; then flunk "panel expands on hover"; echo "$x0_line"; exit 1; fi
x0="${x0_line#x0=}"
pass "window expanded to 452 x 396 (panel 420 x 254, clip room for the 380 pt card) with the panel's left edge at x=$x0"

y_ctl=162; y_bar=124
x_play=$((x0 + 210)); x_prev=$((x0 + 210 - 55)); x_next=$((x0 + 210 + 55))
bar_left=$((x0 + 68)); bar_w=284

# Runs a driver command; exit 2 from it means the environment is wrong, not the app.
drv() {
  local out rc
  out="$("$drive" "$@" 2>&1)"; rc=$?
  [ "$rc" -eq 2 ] && { echo "SKIP $out"; exit 2; }
  [ "$rc" -ne 0 ] && { echo "$out"; return 1; }
  return 0
}

# Hovers the panel open again before a step, so each step starts from an expanded panel whatever the cursor did since.
rehover() {
  local out rc
  out="$("$drive" hover "$app" 2>&1)"; rc=$?
  [ "$rc" -eq 2 ] && { echo "SKIP $out"; exit 2; }
  [ "$rc" -ne 0 ] && { flunk "panel expands again before $1"; echo "$out"; }
  return 0
}

# Checks that the frontmost app is still the one the test started with and that no `act` failed.
steady() {
  local name="$1" now
  now="$("$drive" front)"
  if [ "$now" = "$front0" ]; then pass "$name: frontmost app unchanged ($front0)"; else flunk "$name: frontmost app changed from [$front0] to [$now]"; fi
  if tail -n +$((am + 1)) "$applog" | grep -q 'media: act .* failed'; then flunk "$name: an act failed"; tail -n +$((am + 1)) "$applog" | grep 'media: act .* failed'; fi
}

# step <name> <fixture line> <app line>...: the caller has already done the click and set $fm and $am marks.
expect() {
  local name="$1" fxwant="$2"; shift 2
  if wait_for "$fxlog" 5 "$fxwant" "$fm"; then pass "$name: $fxwant"; else flunk "$name: fixture did not print: $fxwant"; fi
  local w
  for w in "$@"; do
    if wait_for "$applog" 5 "$w" "$am"; then pass "$name: app logged: $w"; else flunk "$name: app did not log: $w"; fi
  done
  steady "$name"
}
mark() { fm="$(lines "$fxlog")"; am="$(lines "$applog")"; }

# 1. play/pause pauses, then plays again.
rehover pause; mark; drv click "$app" "$x_play" "$y_ctl" || flunk "click play/pause"
expect "pause" 'fixture: toggle paused' 'media: act toggle ok' 'media: adapter ok paused "Ctl Song"'

rehover play; mark; drv click "$app" "$x_play" "$y_ctl" || flunk "click play/pause again"
expect "play" 'fixture: toggle playing' 'media: act toggle ok' 'media: adapter ok playing "Ctl Song"'

# 2. next switches to the second track.
rehover next; mark; drv click "$app" "$x_next" "$y_ctl" || flunk "click next"
expect "next" 'fixture: next "Ctl Song II"' 'media: act next ok' 'media: adapter ok playing "Ctl Song II"'

# 3. previous straight away (shown elapsed is under 3 s) is a real previous, not a restart.
rehover previous; mark; drv click "$app" "$x_prev" "$y_ctl" || flunk "click previous"
expect "previous" 'fixture: previous "Ctl Song"' 'media: act previous ok' 'media: adapter ok playing "Ctl Song"'

# 4. a drag on the seek bar from 25 % to 50 % sends exactly one seek, near 50 % of 223000 ms.
rehover seek; mark
"$drive" drag "$app" $((bar_left + bar_w / 4)) $((bar_left + bar_w / 2)) "$y_bar" 12 2>"$tmp/drag.log"; rc=$?
[ "$rc" -eq 2 ] && { echo "SKIP $(cat "$tmp/drag.log")"; exit 2; }
[ "$rc" -ne 0 ] && flunk "drag seek bar"
sed 's/^/note: /' "$tmp/drag.log"
if wait_for "$applog" 5 'media: act seek ' "$am"; then
  sleep 1
  seeks="$(tail -n +$((am + 1)) "$applog" | grep -E '^media: act seek [0-9]+ ok$')"
  count="$(printf '%s\n' "$seeks" | grep -c .)"
  ms="$(printf '%s\n' "$seeks" | head -n 1 | awk '{print $4}')"
  if [ "$count" = 1 ]; then pass "seek: exactly one act seek"; else flunk "seek: $count act seek lines, want 1"; fi
  if [ -n "$ms" ] && [ "$ms" -ge 100350 ] && [ "$ms" -le 122650 ]; then pass "seek: $ms ms is within 45-55 % of 223000"; else flunk "seek: [$ms] ms is outside 100350..122650"; fi
else
  flunk "seek: no 'media: act seek' line"
fi
fxseek="$(tail -n +$((fm + 1)) "$fxlog" | grep -E '^fixture: seek [0-9.]+$' | tail -n 1 | awk '{print $3}')"
if [ -n "$fxseek" ] && awk -v s="$fxseek" 'BEGIN { exit !(s >= 100 && s <= 123) }'; then pass "seek: fixture seeked to $fxseek s"; else flunk "seek: fixture seek [$fxseek] s, want 100..123"; fi
steady "seek"

# 4b. once the shown elapsed is over 3 s, previous restarts the track: a seek to 0, not a previous track. The sleep
# lets the seek's pending change end (elapsed is already far past 3 s after the seek).
sleep 3.5
rehover restart; mark; drv click "$app" "$x_prev" "$y_ctl" || flunk "click previous (restart)"
expect "restart" 'fixture: seek 0.0' 'media: act seek 0 ok'
if tail -n +$((fm + 1)) "$fxlog" | grep -qF 'fixture: previous'; then flunk "restart: the fixture got a previous, not only a seek"; else pass "restart: no previous reached the fixture"; fi

# 4c. a drag that overshoots the panel's right edge: the press latch keeps the panel open while the pointer is outside
# it, so the release still lands, and a position past the end clamps to the duration.
sleep 2.5
rehover overshoot; mark
"$drive" drag "$app" $((bar_left + bar_w * 3 / 4)) $((x0 + 420 + 30)) "$y_bar" 12 2>"$tmp/drag.log"; rc=$?
[ "$rc" -eq 2 ] && { echo "SKIP $(cat "$tmp/drag.log")"; exit 2; }
[ "$rc" -ne 0 ] && flunk "overshoot drag"
sed 's/^/note: /' "$tmp/drag.log"
expect "overshoot" 'fixture: seek 223.0' 'media: act seek 223000 ok'
oseeks="$(tail -n +$((am + 1)) "$applog" | grep -cE '^media: act seek [0-9]+ ok$')"
if [ "$oseeks" = 1 ]; then pass "overshoot: exactly one act seek"; else flunk "overshoot: $oseeks act seek lines, want 1"; fi

# 5. the test app was never activated, and nothing else was either.
sleep 0.5
if grep -q '^activated' "$watchlog"; then flunk "no app activation while driving"; grep '^activated' "$watchlog"; else pass "no app activation while driving"; fi
if grep -q 'media: act .* failed' "$applog"; then flunk "no act failed"; else pass "no act failed"; fi

if [ "$fail" -ne 0 ]; then
  echo "--- app.log"; cat "$applog"; echo "--- fixture.log"; cat "$fxlog"; echo "--- watch.log"; cat "$watchlog"
fi
exit "$fail"
