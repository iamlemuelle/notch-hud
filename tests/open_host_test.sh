#!/bin/bash
# End to end, on the real panel and the real mouse path: the `Open <app>` button of a session with no inbox brings that
# session's app forward and does nothing else (CONTRACT.md section 11, "Opening the session's app"). One session is
# seeded through the real hook with __CFBundleIdentifier=com.apple.TextEdit and no messaging socket, so its record has
# "inbox":null and "host":"com.apple.TextEdit" and the card shows the button. The app that was frontmost when the test
# started is "the other app": it is put back in front after notch-hud (an unbundled binary comes up frontmost) starts.
#
# Case not running: a real click on the button logs `host: not running com.apple.TextEdit` within 3 s, and 2 s later
# TextEdit is still not running (the button never launches an app), no TextEdit activation was seen, and the other app
# is still frontmost. Case running: TextEdit is launched in the background with a decoy document, the other app is put
# in front again, Back and the row reopen the card, and a click on the button logs `host: opened com.apple.TextEdit`
# and makes TextEdit the frontmost app within 2 s (the time is printed). Throughout the app logs no `key: on` (the
# button never takes the keyboard) and notch-hud itself is never activated.
#
# It posts mouse events, so the terminal running it needs Accessibility. Exit 0 when every check passes, 1 when one
# fails, 2 when the environment cannot run it, including TextEdit already running (the not-running case would need
# TextEdit quit, which this test never does to the user's TextEdit) and a failure while someone used the trackpad,
# mouse or keyboard. Do not touch the mouse or the keyboard or switch apps while it runs.
# It runs its own app process with a temp NOTCH_DIR and kills only its own processes; it never touches the installed
# app or ~/.claude. It starts TextEdit itself, closes its decoy and quits TextEdit at the end, and puts the other app
# back in front.
set -u
unset CLAUDE_CODE_MESSAGING_SOCKET CLAUDE_CODE_MESSAGING_TOKEN  # never let a test reach a real session's inbox
root="$(cd "$(dirname "$0")/.." && pwd)"
export PATH="/opt/homebrew/opt/rustup/bin:$PATH"
fail=0
pass() { echo "PASS $1"; }
flunk() { echo "FAIL $1"; fail=1; }

# The not-running case cannot be checked without quitting the user's TextEdit, which this test never does.
if pgrep -xq TextEdit; then echo "SKIP TextEdit is already running; quit it first (this test never quits a TextEdit it did not start)"; exit 2; fi

tmp="$(mktemp -d)"
app=""; watcher=""; hands=""; cursor_x=""; cursor_y=""; te=""; other_bid=""
drive="$tmp/drive"
decoy_name="notch-open-host-decoy-$$.txt"
decoy="$tmp/$decoy_name"
cleanup() {
  [ -n "$cursor_x" ] && "$drive" warp "$cursor_x" "$cursor_y" 2>/dev/null
  if [ -n "$te" ]; then
    "$drive" axclose "$te" "$decoy_name" 2>/dev/null
    "$drive" quit "$te" 2>/dev/null
  fi
  [ -n "$other_bid" ] && open -b "$other_bid" 2>/dev/null
  [ -n "$watcher" ] && kill "$watcher" 2>/dev/null
  [ -n "$hands" ] && kill "$hands" 2>/dev/null
  [ -n "$app" ] && kill "$app" 2>/dev/null
  wait 2>/dev/null
}
trap cleanup EXIT

swiftc -O "$root/tests/controls/Drive.swift" -o "$drive" 2>"$tmp/swiftc.log" || { cat "$tmp/swiftc.log"; echo "FAIL driver build"; exit 2; }
"$drive" access || {
  echo "SKIP needs Accessibility to post events: run from a terminal that has it (the Claude app's Terminal panel)"
  exit 2
}

(cd "$root/src-tauri" && cargo build --release -q) || { echo "FAIL app build"; exit 2; }

export NOTCH_DIR="$tmp/sessions"
export NOTCH_TRACE=1
applog="$tmp/app.log"; watchlog="$tmp/watch.log"; handlog="$tmp/hands.log"
mkdir -p "$NOTCH_DIR"

# Ends the run with code $1, or with 2 when it failed while the hardware sent input (`drive hands`).
finish() {
  if [ "$1" -eq 1 ] && grep -q '^hand ' "$handlog" 2>/dev/null; then
    echo "SKIP the trackpad, mouse or keyboard was used during the run, so a failure is not the app's: $(grep -c '^hand ' "$handlog") events, first: $(grep -m1 '^hand ' "$handlog")"
    exit 2
  fi
  exit "$1"
}

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
since() { tail -n +$(($2 + 1)) "$1"; }
front_bid() { local f; f="$("$drive" front)"; echo "${f#* }"; }
now() { perl -MTime::HiRes=time -e 'printf "%.3f\n", time'; }
# Posted input lands wherever the pointer is, so nothing is posted unless the panel is the open window.
panel_open() {
  local b; b="$("$drive" bounds "$app" 2>&1)"
  case "$b" in *" 452 396") return 0;; esac
  flunk "the panel is not the open window ($1): bounds [$b]; the test stops before posting more input"
  return 1
}
# Puts the other app in front, up to 5 s.
bring_other_front() {
  local i=0
  open -b "$other_bid" 2>/dev/null
  while [ "$i" -lt 25 ]; do
    [ "$(front_bid)" = "$other_bid" ] && return 0
    sleep 0.2; i=$((i + 1))
  done
  return 1
}

# One session through the real hook: no messaging socket, host TextEdit. Row 1 is "hostproj" (session sess-host).
printf '%s' '{"session_id":"sess-host","cwd":"/x/hostproj","hook_event_name":"PreToolUse","tool_name":"Read"}' | env -u CLAUDE_CODE_MESSAGING_SOCKET __CFBundleIdentifier=com.apple.TextEdit "$root/hooks/notch-hook.sh"
rec="$(cat "$NOTCH_DIR"/* 2>/dev/null)"
case "$rec" in *'"inbox":null'*) pass "the seeded record has \"inbox\":null";; *) flunk "the seeded record is [$rec], want \"inbox\":null"; exit 1;; esac
case "$rec" in *'"host":"com.apple.TextEdit"'*) pass "the seeded record has \"host\":\"com.apple.TextEdit\"";; *) flunk "the seeded record is [$rec], want \"host\":\"com.apple.TextEdit\""; exit 1;; esac

read -r cursor_x cursor_y < <("$drive" cursor)
"$drive" hands >"$handlog" 2>&1 & hands=$!
wait_for "$handlog" 5 watching || { echo "SKIP cannot watch for hardware input: $(cat "$handlog")"; exit 2; }

other="$("$drive" front)"
other_bid="${other#* }"
case "$other_bid" in ""|com.apple.TextEdit|com.lem.notch-hud) echo "SKIP the frontmost app is [$other], not one to put back in front"; other_bid=""; exit 2;; esac

# The app first, since an unbundled binary comes up frontmost (wry activates it when it builds the webview); the other
# app then takes the front back.
"$root/src-tauri/target/release/notch-hud" 2>"$applog" & app=$!
if ! wait_for "$applog" 15 'page: loaded'; then flunk "page loads"; echo "--- app.log"; cat "$applog"; finish 1; fi
if ! wait_for "$applog" 15 '"total":1'; then flunk "the app sees the session"; echo "--- app.log"; cat "$applog"; finish 1; fi
if bring_other_front; then pass "the other app ($other_bid) is frontmost again after notch-hud started"; else flunk "the other app ($other_bid) did not come back to the front; frontmost is [$(front_bid)]"; finish 1; fi

"$drive" watch >"$watchlog" 2>&1 & watcher=$!
wait_for "$watchlog" 5 watching || { echo "FAIL activation watcher did not start"; exit 2; }

x0_line="$("$drive" hover "$app" 2>&1)"; rc=$?
if [ "$rc" -eq 2 ]; then echo "SKIP $x0_line"; exit 2; fi
if [ "$rc" -ne 0 ]; then flunk "panel expands on hover"; echo "$x0_line"; finish 1; fi
x0="${x0_line#x0=}"
pass "panel expanded with its left edge at x=$x0"

# Row 1 spans 218..240 pt below the panel top (CONTRACT.md section 4).
panel_open "before the row click" || finish 1
am="$(lines "$applog")"
"$drive" click "$app" $((x0 + 120)) 229 >/dev/null 2>&1 || flunk "click row 1"
if wait_for "$applog" 5 'page: detail sess-host found' "$am"; then pass "a click on row 1 opened the card: page: detail sess-host found"; else flunk "no 'page: detail sess-host found' after the row click; the test stops"; finish 1; fi
sleep 0.8

# Case not running. The button: 316..350 pt below the panel top (section 11).
panel_open "before the first button click" || finish 1
w0="$(lines "$watchlog")"
bm="$(lines "$applog")"
"$drive" click "$app" $((x0 + 120)) 333 >/dev/null 2>&1 || flunk "click the button"
if wait_for "$applog" 3 'host: not running com.apple.TextEdit' "$bm"; then pass "TextEdit is not running: host: not running com.apple.TextEdit"; else
  flunk "no 'host: not running com.apple.TextEdit' within 3 s of the click; got: $(since "$applog" "$bm" | grep -F 'host:' | tr '\n' ' ')"; finish 1
fi
sleep 2
if pgrep -xq TextEdit; then flunk "TextEdit is running 2 s after the click: the button launched it"; finish 1; else pass "TextEdit is still not running 2 s later"; fi
if since "$watchlog" "$w0" | grep -q ' com.apple.TextEdit$'; then flunk "TextEdit was activated by the click in the not-running case"; finish 1; else pass "no TextEdit activation was seen"; fi
f="$(front_bid)"
if [ "$f" = "$other_bid" ]; then pass "the other app ($other_bid) is still frontmost"; else flunk "frontmost app is [$f] after the click, want $other_bid"; finish 1; fi

# Case running: TextEdit in the background with a decoy document (an app with no window is terminated at once).
: >"$decoy"
open -g -a TextEdit "$decoy" || { echo "SKIP cannot open TextEdit"; exit 2; }
for _ in $(seq 1 50); do
  te="$(pgrep -x TextEdit | head -n 1)"
  [ -n "$te" ] && break
  sleep 0.2
done
[ -n "$te" ] || { echo "SKIP TextEdit did not start"; exit 2; }
sleep 1
if bring_other_front; then pass "the other app is frontmost with TextEdit running in the background"; else flunk "the other app did not come back to the front; frontmost is [$(front_bid)]; the test stops"; finish 1; fi

# Back (196..214 pt below the panel top, from x 14), then row 1 again, so the card is drawn with TextEdit running.
panel_open "before Back" || finish 1
"$drive" click "$app" $((x0 + 40)) 205 >/dev/null 2>&1 || flunk "click Back"
sleep 0.8
panel_open "before the second row click" || finish 1
am="$(lines "$applog")"
"$drive" click "$app" $((x0 + 120)) 229 >/dev/null 2>&1 || flunk "click row 1 again"
if wait_for "$applog" 5 'page: detail sess-host found' "$am"; then pass "Back and row 1 opened the card again"; else flunk "no 'page: detail sess-host found' after Back and a row click; the test stops"; finish 1; fi
sleep 0.8

panel_open "before the second button click" || finish 1
if [ "$(front_bid)" != "$other_bid" ]; then flunk "frontmost app is [$(front_bid)] before the second click, want $other_bid; the test stops"; finish 1; fi
w1="$(lines "$watchlog")"
bm="$(lines "$applog")"
t0="$(now)"
"$drive" click "$app" $((x0 + 120)) 333 >/dev/null 2>&1 || flunk "click the button again"
front_ms=""
for _ in $(seq 1 20); do
  if [ "$(front_bid)" = "com.apple.TextEdit" ]; then front_ms="$(perl -e "printf '%d', ($(now) - $t0) * 1000")"; break; fi
  sleep 0.1
done
if [ -n "$front_ms" ] && [ "$front_ms" -le 2000 ]; then pass "TextEdit was frontmost ${front_ms} ms after the click"; else flunk "TextEdit was not frontmost within 2 s of the click (frontmost [$(front_bid)])"; fi
if wait_for "$applog" 3 'host: opened com.apple.TextEdit' "$bm"; then pass "host: opened com.apple.TextEdit"; else flunk "no 'host: opened com.apple.TextEdit'; got: $(since "$applog" "$bm" | grep -F 'host:' | tr '\n' ' ')"; fi
echo "INFO frontmost ${front_ms:-none} ms after the click"

if grep -qF 'key: on' "$applog"; then flunk "the button took the keyboard: key: on in the app log"; else pass "no key: on in the app log"; fi
if grep -q "^activated $app " "$watchlog"; then flunk "notch-hud was activated: $(grep "^activated $app " "$watchlog" | head -n 1)"; else pass "notch-hud was never activated"; fi
others="$(sed -n "$((w1 + 1)),\$p" "$watchlog" | grep '^activated' | grep -v ' com.apple.TextEdit$')"
if [ -n "$others" ]; then flunk "an app other than TextEdit was activated by the second click: $(echo $others)"; fi

if [ "$fail" -ne 0 ]; then echo "--- app.log"; cat "$applog"; echo "--- watch.log"; cat "$watchlog"; fi
finish "$fail"
