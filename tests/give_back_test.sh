#!/bin/bash
# End to end, on the real panel and the real keyboard path: once the prompt box has had the keyboard and given it back,
# a plain click on the panel does not take the keyboard again (CONTRACT.md section 3 "Key focus" and section 11). The
# session's inbox is the fake of tests/lib/fake_inbox.sh. TextEdit is frontmost with an empty document as a decoy. A
# real click opens a session's card and another clicks the box (`key: on`); posted keys `hello` go to the box, Esc gives
# the keyboard back (`key: off`) with the pointer still on the panel, and posted key `a` lands in the decoy. A click on
# the card's Folder row, not the box, then logs neither `key: on` nor `key: off`, the panel stays open, and posted key
# `z` lands in the decoy too (`az`): before the fix the click made the panel key and `z` was lost. Nothing reaches the
# fake inbox, and no app other than TextEdit is activated.
#
# It posts mouse and key events, so the terminal running it needs Accessibility. Exit 0 when every check passes, 1 when
# one fails, 2 when the environment cannot run it. Do not touch the mouse or the keyboard or switch apps while it runs.
# It runs its own app process with a temp NOTCH_DIR and kills only that process; it never touches the installed app. It
# opens its decoy in TextEdit, closes that document at the end, and quits TextEdit only if it was not running before.
set -u
unset CLAUDE_CODE_MESSAGING_SOCKET CLAUDE_CODE_MESSAGING_TOKEN  # never let a test reach a real session's inbox
root="$(cd "$(dirname "$0")/.." && pwd)"
export PATH="/opt/homebrew/opt/rustup/bin:$PATH"
fail=0
pass() { echo "PASS $1"; }
flunk() { echo "FAIL $1"; fail=1; }

tmp="$(mktemp -d)"
app=""; watcher=""; inbox_pid=""; cursor_x=""; cursor_y=""; te=""; te_started=0
drive="$tmp/drive"
decoy_name="notch-give-back-decoy-$$.txt"
decoy="$tmp/$decoy_name"
sock="$tmp/inbox.sock"
received="$tmp/received"
cleanup() {
  [ -n "$cursor_x" ] && "$drive" warp "$cursor_x" "$cursor_y" 2>/dev/null
  if [ -n "$te" ]; then
    "$drive" axclose "$te" "$decoy_name" 2>/dev/null
    [ "$te_started" -eq 1 ] && "$drive" quit "$te" 2>/dev/null
  fi
  [ -n "$watcher" ] && kill "$watcher" 2>/dev/null
  [ -n "$app" ] && kill "$app" 2>/dev/null
  [ -n "$inbox_pid" ] && kill "$inbox_pid" 2>/dev/null
  wait 2>/dev/null
}
trap cleanup EXIT

source "$root/tests/lib/fake_inbox.sh"
fake_inbox_check_path

swiftc -O "$root/tests/controls/Drive.swift" -o "$drive" 2>"$tmp/swiftc.log" || { cat "$tmp/swiftc.log"; echo "FAIL driver build"; exit 2; }
"$drive" access || {
  echo "SKIP needs Accessibility to post events: run from a terminal that has it (the Claude app's Terminal panel)"
  exit 2
}

(cd "$root/src-tauri" && cargo build --release -q) || { echo "FAIL app build"; exit 2; }

export NOTCH_DIR="$tmp/sessions"
export NOTCH_TRACE=1
applog="$tmp/app.log"; watchlog="$tmp/watch.log"; inboxlog="$tmp/inbox.log"
mkdir -p "$NOTCH_DIR"

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
front_is_textedit() {
  local f; f="$("$drive" front)"
  if [ "${f#* }" = "com.apple.TextEdit" ]; then pass "frontmost app is TextEdit $1"; else flunk "frontmost app is [$f] $1, want TextEdit"; fi
}

start_fake_inbox

# One session through the real hook, with the fake as its inbox: row 1 is "typeproj" (session sess-type).
printf '%s' '{"session_id":"sess-type","cwd":"/x/typeproj","hook_event_name":"PreToolUse","tool_name":"Read"}' | env -u __CFBundleIdentifier CLAUDE_CODE_MESSAGING_SOCKET="$sock" "$root/hooks/notch-hook.sh"

read -r cursor_x cursor_y < <("$drive" cursor)

# The app first, since an unbundled binary comes up frontmost (wry activates it when it builds the webview); TextEdit
# then takes the front.
"$root/src-tauri/target/release/notch-hud" 2>"$applog" & app=$!
if ! wait_for "$applog" 15 'page: loaded'; then flunk "page loads"; echo "--- app.log"; cat "$applog"; exit 1; fi
if ! wait_for "$applog" 15 '"total":1'; then flunk "the app sees the session"; echo "--- app.log"; cat "$applog"; exit 1; fi

pgrep -xq TextEdit || te_started=1
: >"$decoy"
open -a TextEdit "$decoy" || { echo "SKIP cannot open TextEdit"; exit 2; }
for _ in $(seq 1 50); do
  f="$("$drive" front)"
  if [ "${f#* }" = "com.apple.TextEdit" ]; then te="${f%% *}"; "$drive" axtext "$te" "$decoy_name" >/dev/null 2>&1 && break; fi
  sleep 0.2
done
[ -n "$te" ] && text0="$("$drive" axtext "$te" "$decoy_name" 2>&1)" || { echo "SKIP TextEdit did not come up frontmost with the decoy"; exit 2; }
if [ "$text0" = "" ]; then pass "TextEdit is frontmost (pid $te) with the empty decoy $decoy_name"; else flunk "the decoy is not empty: [$text0]"; exit 1; fi

"$drive" watch >"$watchlog" 2>&1 & watcher=$!
wait_for "$watchlog" 5 watching || { echo "FAIL activation watcher did not start"; exit 2; }

x0_line="$("$drive" hover "$app" 2>&1)"; rc=$?
if [ "$rc" -eq 2 ]; then echo "SKIP $x0_line"; exit 2; fi
if [ "$rc" -ne 0 ]; then flunk "panel expands on hover"; echo "$x0_line"; exit 1; fi
x0="${x0_line#x0=}"
pass "panel expanded with its left edge at x=$x0"

# Row 1 spans 218..240 pt below the panel top (CONTRACT.md section 4).
am="$(lines "$applog")"
"$drive" click "$app" $((x0 + 120)) 229 >/dev/null 2>&1 || flunk "click row 1"
if wait_for "$applog" 5 'page: detail sess-type found' "$am"; then pass "a click on row 1 opened the card: page: detail sess-type found"; else flunk "no 'page: detail sess-type found' after the row click"; exit 1; fi
sleep 0.8

# The box: card top 196, header 18, 4 pt, four 22 pt rows, 10 pt, so 316..350 pt below the panel top.
km="$(lines "$applog")"
"$drive" click "$app" $((x0 + 120)) 333 >/dev/null 2>&1 || flunk "click the box"
if wait_for "$applog" 3 'key: on' "$km"; then pass "a click on the box made the panel key: key: on"; else
  # Without the keyboard, the keys below would go to whatever app is in front.
  flunk "no 'key: on' after the click on the box; the test stops before posting keys"; exit 1
fi
front_is_textedit "after the panel took the keyboard"
"$drive" type hello

# Esc with the pointer still on the panel, so the card stays open for the click below.
em="$(lines "$applog")"
"$drive" key escape
if wait_for "$applog" 3 'key: off' "$em"; then pass "Esc gave the keyboard back: key: off"; else flunk "no 'key: off' after Esc; the test stops"; exit 1; fi
sleep 0.5
"$drive" type a
sleep 0.5
text1="$("$drive" axtext "$te" "$decoy_name" 2>&1)"
if [ "$text1" = "a" ]; then pass "a key after Esc landed in TextEdit: the decoy reads a"; else flunk "the decoy reads [$text1] after Esc, want a; the test stops"; exit 1; fi

# The Folder row, the card's second row: 240..262 pt below the panel top. Not the box, so typing must not start.
cm="$(lines "$applog")"
"$drive" click "$app" $((x0 + 120)) 251 >/dev/null 2>&1 || flunk "click the Folder row"
sleep 0.6
if since "$applog" "$cm" | grep -qF 'key: '; then flunk "the Folder row click changed the keyboard: $(since "$applog" "$cm" | grep -F 'key: ' | tr '\n' ' ')"; else pass "the Folder row click logged neither key: on nor key: off"; fi
b="$("$drive" bounds "$app")"
case "$b" in *" 452 396") pass "the panel is still the open window after the click: $b";; *) flunk "window bounds are [$b] after the Folder row click, want w=452 h=396";; esac
"$drive" type z
sleep 0.5
text2="$("$drive" axtext "$te" "$decoy_name" 2>&1)"
if [ "$text2" = "az" ]; then pass "a key after the Folder row click landed in TextEdit: the decoy reads az"; else flunk "the decoy reads [$text2] after the Folder row click, want az (the click took the keyboard)"; fi
front_is_textedit "after the Folder row click"

# The pointer leaves: the panel collapses as usual, so ordering it out and back in kept its hover tracking.
hm="$(lines "$applog")"
"$drive" move $((x0 + 210)) 640
if wait_for "$applog" 3 'window: pill' "$hm"; then pass "the pointer left and the panel collapsed: window: pill"; else flunk "no 'window: pill' within 3 s of the pointer leaving"; fi

if [ -s "$received" ]; then flunk "the fake inbox received [$(cat "$received")], want nothing"; else pass "nothing reached the fake inbox"; fi
if grep '^activated' "$watchlog" | grep -qv ' com.apple.TextEdit$'; then flunk "an app other than TextEdit was activated"; grep '^activated' "$watchlog"; else pass "no app other than TextEdit was activated"; fi
echo "INFO key: on lines: $(grep -c '^key: on' "$applog"), key: off lines: $(grep -c '^key: off' "$applog")"

if [ "$fail" -ne 0 ]; then echo "--- app.log"; cat "$applog"; echo "--- watch.log"; cat "$watchlog"; fi
exit "$fail"
