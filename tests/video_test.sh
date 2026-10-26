#!/bin/bash
# End to end, on the real release app: a video file dragged onto the pill opens the video stage, the stage is pinned
# against the pointer leaving, its close button gives the open window back and then the pill, and a file that is not a
# video is rejected and the pill returns (CONTRACT.md sections 8 "Theater" and 12). The app is never activated.
#
# It MOVES THE REAL CURSOR and posts mouse events, and it drags a file out of a small window of its own. The terminal
# running it needs Accessibility. It restores the cursor on exit. Do not touch the mouse or switch apps while it runs.
# It runs its own app process with a temp NOTCH_DIR and signals only that process and its own helpers; the notch-hud
# that may be running as your login item is left alone (windows are filtered by the test app's pid).
#
# Exit 0 when every check passes, 1 when one fails, 2 when the environment cannot run it, including a failure while
# someone used the trackpad, mouse or keyboard.
set -u
unset CLAUDE_CODE_MESSAGING_SOCKET CLAUDE_CODE_MESSAGING_TOKEN  # never let a test reach a real session's inbox
root="$(cd "$(dirname "$0")/.." && pwd)"
export PATH="/opt/homebrew/opt/rustup/bin:$PATH"
fail=0
pass() { echo "PASS $1"; }
flunk() { echo "FAIL $1"; fail=1; }

tmp="$(mktemp -d)"
app=""; hands=""; watcher=""; cursor_x=""; cursor_y=""
drive="$tmp/drive"
clip="$root/tests/video/fixtures/clip.mp4"
notes="$tmp/notes.txt"
cleanup() {
  [ -n "$cursor_x" ] && "$drive" warp "$cursor_x" "$cursor_y" 2>/dev/null
  [ -n "$hands" ] && kill "$hands" 2>/dev/null
  [ -n "$watcher" ] && kill "$watcher" 2>/dev/null
  [ -n "$app" ] && kill "$app" 2>/dev/null
  wait 2>/dev/null
}
trap cleanup EXIT

[ -f "$clip" ] || { echo "FAIL no fixture clip at $clip"; exit 2; }
swiftc -O "$root/tests/controls/Drive.swift" -o "$drive" 2>"$tmp/swiftc.log" || { cat "$tmp/swiftc.log"; echo "FAIL driver build"; exit 2; }
"$drive" access || {
  echo "SKIP needs Accessibility to post events: run from a terminal that has it (the Claude app's Terminal panel)"
  exit 2
}
read -r notch_w notch_h < <("$drive" notch) || { echo "SKIP no notched display"; exit 2; }
read -r screen_x screen_y screen_w screen_h < <("$drive" screen) || { echo "SKIP no notched display"; exit 2; }

# Window sizes of CONTRACT.md section 8 for this notch and screen. pill = notch + 2 * 60 ears. The stage is
# w = min(608, screen width / 2) wide, with a 16:9 picture (w - 32) wide: its window is w + 32 wide and
# max(notch + 8 + picture + 8 + 36 + 12, 380, the open height for the seeded sessions) + 16 tall: 640 x (notch + 404)
# unless the screen is narrower than 1216, floored at 396. The script seeds one session, whose open height is 254, so
# the floor is 380. The open window is 452 x 396.
pill_w=$((notch_w + 120)); pill_h=$notch_h
open_w=452; open_h=396
theater_w="$(awk -v s="$screen_w" 'BEGIN { w = s / 2; if (w > 608) w = 608; printf "%.10g", w }')"
picture_h="$(awk -v w="$theater_w" 'BEGIN { printf "%.10g", (w - 32) * 9 / 16 }')"
win_w="$(awk -v w="$theater_w" 'BEGIN { printf "%d", w + 32 + 0.5 }')"
win_h="$(awk -v n="$notch_h" -v p="$picture_h" 'BEGIN { h = n + 8 + p + 8 + 36 + 12; if (h < 380) h = 380; printf "%d", h + 16 + 0.5 }')"
if [ "$theater_w" = 608 ]; then theater_log="window: theater 640(\.0+)?x$((notch_h + 404))(\.0+)?"; else theater_log="window: theater [0-9.]+x[0-9.]+"; fi

(cd "$root/src-tauri" && cargo build --release -q) || { echo "FAIL app build"; exit 2; }

export NOTCH_DIR="$tmp/sessions"
export NOTCH_TRACE=1
applog="$tmp/app.log"; handlog="$tmp/hands.log"; watchlog="$tmp/watch.log"
: >"$applog"
printf 'not a video\n' >"$notes"

# Ends the run with code $1, or with 2 when it failed while the hardware sent input (`drive hands`).
finish() {
  if [ "$1" -eq 1 ] && grep -q '^hand ' "$handlog" 2>/dev/null; then
    echo "SKIP the trackpad, mouse or keyboard was used during the run, so a failure is not the app's: $(grep -c '^hand ' "$handlog") events, first: $(grep -m1 '^hand ' "$handlog")"
    exit 2
  fi
  exit "$1"
}

# Seeds one working session through the real hook.
hook() { printf '%s' "$1" | "$root/hooks/notch-hook.sh"; }
hook '{"session_id":"vt1","cwd":"/x/video","hook_event_name":"PreToolUse","tool_name":"Read","tool_input":{}}'
[ -f "$NOTCH_DIR/vt1.json" ] || { echo "FAIL the hook did not write the session record (is jq installed?)"; exit 2; }

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
# As wait_for with an extended regular expression.
wait_for_re() {
  local log="$1" secs="$2" want="$3" after="${4:-0}" i=0
  while [ "$i" -lt $((secs * 5)) ]; do
    tail -n +$((after + 1)) "$log" | grep -qE -- "$want" && return 0
    sleep 0.2; i=$((i + 1))
  done
  return 1
}
lines() { wc -l <"$1" | tr -d ' '; }
since() { tail -n +$(($2 + 1)) "$1"; }
# line_of <log> <extended regex> <after line>: the line number (counted from the mark) of the first match.
line_of() { tail -n +$(($3 + 1)) "$1" | grep -nE -- "$2" | head -n 1 | cut -d: -f1; }
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
away() { "$drive" warp $((screen_x + screen_w / 2)) $((screen_y + screen_h / 2)); }

read -r cursor_x cursor_y < <("$drive" cursor)
"$drive" hands >"$handlog" 2>&1 & hands=$!
wait_for "$handlog" 5 watching || { echo "SKIP cannot watch for hardware input: $(cat "$handlog")"; exit 2; }

# The app's stderr goes through a timestamper: every line is prefixed with the time it was read.
"$root/src-tauri/target/release/notch-hud" 2> >(perl -MTime::HiRes=time -ne 'BEGIN { $| = 1 } printf "%.3f %s", time, $_' >"$applog") & app=$!
if ! wait_for "$applog" 15 'page: loaded'; then flunk "page loads"; echo "--- app.log"; cat "$applog"; finish 1; fi
if ! wait_for "$applog" 15 '"total":1'; then flunk "the app sees the session"; echo "--- app.log"; cat "$applog"; finish 1; fi
pass "page loaded and the app sees the session"

"$drive" watch >"$watchlog" 2>&1 & watcher=$!
wait_for "$watchlog" 5 watching || { echo "FAIL activation watcher did not start"; exit 2; }
sleep 1.5
away
if ! wait_size "$pill_w" "$pill_h" 3; then flunk "collapsed window is the pill: got [$last], want $pill_w x $pill_h"; echo "--- app.log"; cat "$applog"; finish 1; fi
read -r pill_x pill_y _ _ < <(bounds)
target_x=$((pill_x + pill_w / 2)); target_y=$((pill_y + 10))

# a. drag the clip onto the pill: the panel opens, the file is accepted, the window becomes the theater window.
am="$(lines "$applog")"
out="$("$drive" dragfile "$app" "$clip" "$target_x" "$target_y" 2>&1)"; rc=$?
if [ "$rc" -eq 0 ] && [ "$out" = dropped ]; then pass "the app took the dropped clip"; else flunk "dragfile: exit $rc, output [$out]"; echo "--- app.log"; since "$applog" "$am"; finish 1; fi
wait_for_re "$applog" 5 "$theater_log" "$am" || true
d="$(line_of "$applog" 'drag: in' "$am")"; v="$(line_of "$applog" 'video: opened clip\.mp4$' "$am")"; w="$(line_of "$applog" "$theater_log" "$am")"
if [ -n "$d" ] && [ -n "$v" ] && [ -n "$w" ] && [ "$d" -lt "$v" ] && [ "$v" -lt "$w" ]; then
  pass "log: drag: in, video: opened clip.mp4, then $(since "$applog" "$am" | grep -oE 'window: theater [0-9.]+x[0-9.]+' | head -n 1)"
else
  flunk "log: want drag: in [$d], video: opened clip.mp4 [$v], window: theater [$w] in that order"; since "$applog" "$am"
fi
if wait_size "$win_w" "$win_h" 2; then pass "window is the theater window, $win_w x $win_h"; else flunk "window is not the theater window $win_w x $win_h: [$last]"; fi
read -r th_x th_y _ _ < <(bounds)

# b. pinned: the cursor leaves and the stage stays.
am="$(lines "$applog")"
away
sleep 1.5
if wait_size "$win_w" "$win_h" 0.2 && ! since "$applog" "$am" | grep -qE 'window: (open|pill)'; then pass "pinned: the cursor left and the window is still $win_w x $win_h after 1.5 s, no window: open or pill"; else flunk "pinned: the window changed after the cursor left: [$last]"; since "$applog" "$am"; fi

# c. close with a real click on the x button: the last 34 pt column of the control row, whose right edge is the
# picture's right edge, w from the window's left edge (the picture starts 32 in), so its centre is w - 17 across; the
# row is notch + 8 + picture + 8 below the top, 18 to the middle.
close_x="$(awk -v x="$th_x" -v w="$theater_w" 'BEGIN { printf "%d", x + w - 17 + 0.5 }')"
close_y="$(awk -v y="$th_y" -v n="$notch_h" -v p="$picture_h" 'BEGIN { printf "%d", y + n + 8 + p + 8 + 18 + 0.5 }')"
am="$(lines "$applog")"
out="$("$drive" click "$app" "$close_x" "$close_y" 2>&1)" || { flunk "click the close button at ($close_x, $close_y): $out"; finish 1; }
away
o="$(wait_for_re "$applog" 5 "window: open $open_w(\.0+)?x$open_h(\.0+)?" "$am" && line_of "$applog" "window: open $open_w(\.0+)?x$open_h(\.0+)?" "$am")"
wait_for "$applog" 5 'window: pill' "$am" || true
p="$(line_of "$applog" 'window: pill' "$am")"
if [ -n "$o" ] && [ -n "$p" ] && [ "$o" -lt "$p" ]; then pass "after close and leave: window: open ${open_w}x${open_h}, then window: pill"; else flunk "after close and leave: want window: open ${open_w}x${open_h} [$o] before window: pill [$p]"; since "$applog" "$am"; fi
if wait_size "$pill_w" "$pill_h" 3; then pass "window is the pill again"; else flunk "window did not return to the pill: [$last]"; fi

# d. a file that is not a video: the webview takes the drop, the app rejects it and the pill comes back.
sleep 0.5
am="$(lines "$applog")"
out="$("$drive" dragfile "$app" "$notes" "$target_x" "$target_y" 2>&1)"; rc=$?
away
if [ "$rc" -eq 0 ] && [ "$out" = dropped ]; then pass "the webview took the dropped notes.txt"; else flunk "dragfile notes.txt: exit $rc, output [$out]"; fi
if wait_for "$applog" 5 'video: rejected not an mp4, m4v or mov file' "$am"; then pass "log: video: rejected not an mp4, m4v or mov file"; else flunk "no 'video: rejected not an mp4, m4v or mov file' in the log"; since "$applog" "$am"; fi
if wait_size "$pill_w" "$pill_h" 5; then pass "window is the pill again within 5 s"; else flunk "window did not return to the pill within 5 s: [$last]"; fi
if since "$applog" "$am" | grep -qE 'window: theater|video: opened'; then flunk "a rejected file opened the stage"; since "$applog" "$am"; else pass "no window: theater and no video: opened for notes.txt"; fi

# e. the app was never activated.
if grep -q "^activated $app " "$watchlog"; then flunk "the app was activated"; grep "^activated $app " "$watchlog"; else pass "no activation of the app (pid $app)"; fi

if [ "$fail" -ne 0 ]; then echo "--- app.log"; cat "$applog"; echo "--- watch.log"; cat "$watchlog"; fi
finish "$fail"
