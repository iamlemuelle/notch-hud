#!/bin/bash
# End to end, on the real release app: the panel window is the pill while collapsed, grows to the open window on hover,
# shrinks only after the shape is back, and shows banners in the banner window (CONTRACT.md sections 3 and 8). A banner
# that opens under a still cursor resting outside the pill is not a hover; a 1 pt real mouse move inside it is, and
# opens the panel. The frontmost app never changes.
#
# It MOVES THE REAL CURSOR (a warp onto the pill, to the middle of the screen, and to a rest point below the pill; a
# posted 1 pt mouse move, which needs Accessibility access) and restores it on exit. It never clicks. Do not touch the
# mouse or switch apps while it runs. It starts its own app, fixture player and watcher and only ever signals those;
# the notch-hud that may be running as your login item is left alone (windows are filtered by the test app's pid).
#
# Exit 0 when every check passes, 1 when one fails, 2 when the environment cannot run it.
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
read -r notch_w notch_h < <("$drive" notch) || { echo "SKIP no notched display"; exit 2; }
read -r screen_x screen_y screen_w screen_h < <("$drive" screen) || { echo "SKIP no notched display"; exit 2; }

# Window sizes of CONTRACT.md section 8 for this notch: pill = notch + 2 * 60 ears; banner = 380 + 2 * 16 wide and
# banner height (notch + 46) + 16 tall; open = 420 + 2 * 16 = 452 wide, max(open height, detail height 380) + 16 tall. The open height is
# 196 + 18 + 4 + 22 * shown_rows + 14 whatever the media is (254 for one row), so the window is always 396 tall.
pill_w=$((notch_w + 120)); pill_h=$notch_h
banner_w=412; banner_h=$((notch_h + 62))
open_w=452; open_h=396

"$root/scripts/build-adapter.sh" >/dev/null || { echo "FAIL adapter build"; exit 2; }
adapter_dir="$root/src-tauri/target/adapter"
fixture_app="$("$root/tests/media/build-fixture.sh")" || { echo "FAIL fixture build"; exit 2; }
fixture="$fixture_app/Contents/MacOS/NowPlayingFixture"
(cd "$root/src-tauri" && cargo build --release -q) || { echo "FAIL app build"; exit 2; }

export NOTCH_DIR="$tmp/sessions"
export NOTCH_TRACE=1
fxlog="$tmp/fixture.log"; applog="$tmp/app.log"; watchlog="$tmp/watch.log"
: >"$fxlog"; : >"$applog"

# Seeds one working session through the real hook.
hook() { printf '%s' "$1" | "$root/hooks/notch-hook.sh"; }
hook '{"session_id":"mt1","cwd":"/x/motion","hook_event_name":"PreToolUse","tool_name":"Read","tool_input":{}}'
[ -f "$NOTCH_DIR/mt1.json" ] || { echo "FAIL the hook did not write the session record (is jq installed?)"; exit 2; }

now() { perl -MTime::HiRes=time -e 'printf "%.3f", time'; }
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
# The app's panel window as "x y w h" ("none" when it has no window).
bounds() { "$drive" bounds "$app" 2>/dev/null || echo none; }
# wait_size <w> <h> <secs>: polls the panel window every 50 ms until it is w x h (whole points, as drive prints them);
# sets $last to the last bounds.
last=""
wait_size() {
  local w="$1" h="$2" end b bx by bw bh
  end="$(awk -v n="$(now)" -v s="$3" 'BEGIN { printf "%.3f", n + s }')"
  while :; do
    b="$(bounds)"; last="$b"
    if [ "$b" != none ]; then
      read -r bx by bw bh <<<"$b"
      [ "$bw" = "$w" ] && [ "$bh" = "$h" ] && return 0
    fi
    awk -v n="$(now)" -v e="$end" 'BEGIN { exit !(n > e) }' && return 1
    sleep 0.05
  done
}
# ts_of <log> <fixed string> <after line>: the timestamp of the first matching line after the line number.
ts_of() { tail -n +$(($3 + 1)) "$1" | grep -F -- "$2" | head -n 1 | awk '{print $1}'; }
# line_of <log> <extended regex> <after line>: the line number (counted from the mark) of the first match.
line_of() { tail -n +$(($3 + 1)) "$1" | grep -nE -- "$2" | head -n 1 | cut -d: -f1; }

read -r cursor_x cursor_y < <("$drive" cursor)

"$fixture" "Motion Song" "Motion Artist" square 223 74 >"$fxlog" 2>&1 & fixture_pid=$!
# The app's stderr goes through a timestamper: every line is prefixed with the time it was read.
"$root/src-tauri/target/release/notch-hud" 2> >(perl -MTime::HiRes=time -ne 'BEGIN { $| = 1 } printf "%.3f %s", time, $_' >"$applog") & app=$!

if ! wait_for "$applog" 15 'page: loaded'; then flunk "page loads"; echo "--- app.log"; cat "$applog"; exit 1; fi
if ! wait_for "$applog" 15 'media: adapter ok playing "Motion Song" (dev.lem.notchhud.fixture)'; then
  flunk "app sees the fixture"; echo "--- app.log"; cat "$applog"; exit 1
fi
pass "page loaded and the fixture is playing"

"$drive" watch >"$watchlog" 2>&1 & watcher=$!
wait_for "$watchlog" 5 watching || { echo "FAIL activation watcher did not start"; exit 2; }
sleep 1.5
front0="$("$drive" front)"

# 1. collapsed: the window is the pill, and it stays the pill (the first media and the first sessions show no banner).
if wait_size "$pill_w" "$pill_h" 3; then
  steady=1
  for _ in 1 2 3 4 5 6 7 8; do
    b="$(bounds)"; set -- $b
    [ "$3" = "$pill_w" ] && [ "$4" = "$pill_h" ] || { steady=0; last="$b"; }
    sleep 0.2
  done
  if [ "$steady" = 1 ]; then pass "collapsed window is the pill, $pill_w x $pill_h, and stays that for 1.6 s"; else flunk "collapsed window left the pill size: $last (want $pill_w x $pill_h)"; fi
else
  flunk "collapsed window is the pill: got [$last], want $pill_w x $pill_h"
fi
read -r pill_x pill_y _ _ < <(bounds)
target_x=$((pill_x + pill_w / 2)); target_y=$((pill_y + 10))

# 2. warp onto the pill (no click): the window becomes the open window and the log has hover in, then window open.
am="$(lines "$applog")"
"$drive" warp "$target_x" "$target_y"
if wait_size "$open_w" "$open_h" 1; then pass "window is $open_w x $open_h within 1 s of the cursor reaching the pill"; else flunk "window did not reach $open_w x $open_h within 1 s: [$last]"; fi
sleep 0.4
hi="$(line_of "$applog" 'hover: in' "$am")"; wo="$(line_of "$applog" "window: open $open_w(\.0+)?x$open_h(\.0+)?" "$am")"
if [ -n "$hi" ] && [ -n "$wo" ] && [ "$hi" -lt "$wo" ]; then pass "log: hover: in, then window: open ${open_w}x${open_h}"; else flunk "log: hover: in (line [$hi]) must come before window: open ${open_w}x${open_h} (line [$wo])"; tail -n +$((am + 1)) "$applog"; fi
sleep 0.6

# 3. warp away: the window returns to the pill, not sooner than grace 250 + 80 + 200 ms after hover out.
am="$(lines "$applog")"
"$drive" warp $((screen_x + screen_w / 2)) $((screen_y + screen_h / 2))
if wait_size "$pill_w" "$pill_h" 3; then pass "window is the pill again after the cursor left"; else flunk "window did not return to $pill_w x $pill_h: [$last]"; fi
sleep 0.3
t_out="$(ts_of "$applog" 'hover: out' "$am")"
t_pill="$(tail -n +$((am + 1)) "$applog" | grep -E "window: pill " | head -n 1 | awk '{print $1}')"
if [ -n "$t_out" ] && [ -n "$t_pill" ] && awk -v a="$t_out" -v b="$t_pill" 'BEGIN { exit !(b - a >= 0.5) }'; then
  pass "log: window: pill came $(awk -v a="$t_out" -v b="$t_pill" 'BEGIN { printf "%.0f", (b - a) * 1000 }') ms after hover: out (at least 500)"
else
  flunk "log: window: pill [$t_pill] must come at least 500 ms after hover: out [$t_out]"; tail -n +$((am + 1)) "$applog"
fi

# 4. a session needs input: banner window, then the pill again.
am="$(lines "$applog")"
hook '{"session_id":"mt1","cwd":"/x/motion","hook_event_name":"Notification","notification_type":"elicitation_dialog"}'
if wait_size "$banner_w" "$banner_h" 2; then
  pass "elicitation: window is the banner, $banner_w x $banner_h, within 2 s"
  if wait_size "$pill_w" "$pill_h" 4; then pass "elicitation: window is the pill again within 4 s"; else flunk "elicitation: window did not return to the pill: [$last]"; fi
else
  flunk "elicitation: window did not become the banner $banner_w x $banner_h within 2 s: [$last]"; tail -n +$((am + 1)) "$applog"
fi
sleep 0.5

# 5. a song change (the adapter's `next` reaches the fixture): banner window, then the pill again.
fm="$(lines "$fxlog")"; am="$(lines "$applog")"
/usr/bin/perl "$adapter_dir/mediaremote-adapter.pl" "$adapter_dir/MediaRemoteAdapter.framework" "$adapter_dir/MediaRemoteAdapterTestClient" send 4 >/dev/null 2>&1
if wait_for "$fxlog" 5 'fixture: next "Motion Song II"' "$fm"; then pass "fixture received next"; else flunk "fixture did not print: next \"Motion Song II\""; fi
if wait_size "$banner_w" "$banner_h" 3; then
  pass "song change: window is the banner, $banner_w x $banner_h"
  if wait_size "$pill_w" "$pill_h" 4; then pass "song change: window is the pill again within 4 s"; else flunk "song change: window did not return to the pill: [$last]"; fi
else
  flunk "song change: window did not become the banner within 3 s: [$last]"; tail -n +$((am + 1)) "$applog"
fi

# 5b. a banner that opens under a cursor resting inside it but outside the pill is not a hover; a 1 pt move is.
hook '{"session_id":"mt1","cwd":"/x/motion","hook_event_name":"PreToolUse","tool_name":"Read","tool_input":{}}'
sleep 0.5
rest_x=$((pill_x + pill_w / 2)); rest_y=$((pill_y + pill_h + 20))
"$drive" warp "$rest_x" "$rest_y"
sleep 1
am="$(lines "$applog")"
hook '{"session_id":"mt1","cwd":"/x/motion","hook_event_name":"Notification","notification_type":"elicitation_dialog"}'
if wait_size "$banner_w" "$banner_h" 2; then
  pass "resting cursor: window is the banner, $banner_w x $banner_h"
  sleep 0.8
  if tail -n +$((am + 1)) "$applog" | grep -qF 'hover: in'; then flunk "resting cursor: hover: in logged while the cursor only rested in the banner"; tail -n +$((am + 1)) "$applog"; else pass "resting cursor: no hover: in for 0.8 s after the banner opened"; fi
  mm="$(lines "$applog")"
  "$drive" move $((rest_x + 1)) "$rest_y"
  if wait_size "$open_w" "$open_h" 1; then pass "1 pt move in the banner: window is $open_w x $open_h within 1 s"; else flunk "1 pt move in the banner: window did not reach $open_w x $open_h within 1 s: [$last]"; fi
  if tail -n +$((mm + 1)) "$applog" | grep -qF 'hover: in'; then pass "1 pt move in the banner: hover: in logged"; else flunk "1 pt move in the banner: no hover: in logged"; tail -n +$((am + 1)) "$applog"; fi
else
  flunk "resting cursor: window did not become the banner $banner_w x $banner_h within 2 s: [$last]"; tail -n +$((am + 1)) "$applog"
fi
"$drive" warp $((screen_x + screen_w / 2)) $((screen_y + screen_h / 2))
if wait_size "$pill_w" "$pill_h" 4; then pass "resting cursor: window is the pill again after the cursor left"; else flunk "resting cursor: window did not return to the pill: [$last]"; fi
sleep 0.5

# 6. the frontmost app never changed and the app was never activated.
sleep 0.5
front1="$("$drive" front)"
if [ "$front0" = "$front1" ]; then pass "frontmost app unchanged ($front0)"; else flunk "frontmost app changed from [$front0] to [$front1]"; fi
if grep -q "^activated $app " "$watchlog"; then flunk "the app was activated"; grep "^activated $app " "$watchlog"; else pass "no activation of the app (pid $app)"; fi

if [ "$fail" -ne 0 ]; then
  echo "--- app.log"; cat "$applog"; echo "--- fixture.log"; cat "$fxlog"; echo "--- watch.log"; cat "$watchlog"
fi
exit "$fail"
