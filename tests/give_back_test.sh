#!/bin/bash
# End to end, on the real panel and the real keyboard path: once the prompt box has had the keyboard and given it back,
# a plain click on the panel does not take the keyboard again (CONTRACT.md section 3 "Key focus" and section 11). The
# session's inbox is the fake of tests/lib/fake_inbox.sh. TextEdit is frontmost with an empty document as a decoy. A
# real click opens a session's card and another clicks the box (`key: on`); posted keys `hello` go to the box, Esc gives
# the keyboard back (`key: off`) with the pointer still on the panel, and posted key `a` lands in the decoy. A click on
# the card's Folder row, not the box, then logs neither `key: on` nor `key: off`, the panel stays open, and posted key
# `z` lands in the decoy too (`az`): before the fix the click made the panel key and `z` was lost. Then the box is
# clicked again, `hi` typed, and a press on the Folder row ends typing during that press (`key: off`, no `key: on`);
# posted key `q` lands in the decoy (`azq`). A click on Back while typing ends typing and still closes the card, and
# `w` after a row click lands (`azqw`). A switch to Finder and back while typing resigns the panel, and `e` after a
# Folder row click lands (`azqwe`). The pointer leaving still collapses the panel. Nothing reaches the fake inbox, no
# app but TextEdit is activated, and Finder only during the switch. When the panel has collapsed with the pointer on
# it (a stray `hover: out`, seen with and without the fix), the next round opens the card again first and an INFO
# line says so; the Back round is tried a second time then, and fails when it cannot be judged twice.
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
card_at="$(lines "$applog")"
sleep 0.8

# A `hover: out` while the pointer is on the card (seen with and without the fix; not what this test checks) can
# collapse the panel when typing ends, and collapsing resets it to the list. Every round needs the card, so it is
# opened again then, and the count is reported.
# card_at, set when the card opens, is the log's line count then: a `window: pill` after it means the card is gone,
# even when the panel has expanded again (on the list).
collapses=0
collapsed_since() { since "$applog" "$1" | grep -qF 'window: pill'; }
need_card() {
  local line am
  collapsed_since "$card_at" || return 0
  collapses=$((collapses + 1))
  echo "INFO the panel had collapsed $1 with the pointer on it; opening the card again"
  line="$("$drive" hover "$app" 2>&1)" || { flunk "panel expands on hover again: $line"; exit 1; }
  x0="${line#x0=}"
  am="$(lines "$applog")"
  "$drive" click "$app" $((x0 + 120)) 229 >/dev/null 2>&1
  wait_for "$applog" 5 'page: detail sess-type found' "$am" || { flunk "row 1 did not open the card again"; exit 1; }
  card_at="$(lines "$applog")"
  sleep 0.8
}

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
n="$(since "$applog" "$em" | grep -c '^key: off')"
if [ "$n" -eq 1 ]; then pass "the give-back logged key: off once"; else flunk "the give-back logged key: off $n times, want 1"; fi
"$drive" type a
sleep 0.5
text1="$("$drive" axtext "$te" "$decoy_name" 2>&1)"
if [ "$text1" = "a" ]; then pass "a key after Esc landed in TextEdit: the decoy reads a"; else flunk "the decoy reads [$text1] after Esc, want a; the test stops"; exit 1; fi
need_card "after Esc"

# The Folder row, the card's second row: 240..262 pt below the panel top. Not the box, so typing must not start.
cm="$(lines "$applog")"
"$drive" click "$app" $((x0 + 120)) 251 >/dev/null 2>&1 || flunk "click the Folder row"
sleep 0.6
# A guard only: the click that made the panel key before the fix logged neither line either.
if since "$applog" "$cm" | grep -qF 'key: '; then flunk "the Folder row click started or ended typing: $(since "$applog" "$cm" | grep -F 'key: ' | tr '\n' ' ')"; else pass "the Folder row click did not start typing"; fi
b="$("$drive" bounds "$app")"
case "$b" in *" 452 396") pass "the panel is still the open window after the click: $b";; *) flunk "window bounds are [$b] after the Folder row click, want w=452 h=396";; esac
"$drive" type z
sleep 0.5
text2="$("$drive" axtext "$te" "$decoy_name" 2>&1)"
if [ "$text2" = "az" ]; then pass "a key after the Folder row click landed in TextEdit: the decoy reads az"; else flunk "the decoy reads [$text2] after the Folder row click, want az (the click took the keyboard)"; fi
front_is_textedit "after the Folder row click"

# A press outside the box while typing ends typing during that press, so the give-back runs inside the click.
need_card "before the second round"
km="$(lines "$applog")"
"$drive" click "$app" $((x0 + 120)) 333 >/dev/null 2>&1 || flunk "click the box again"
if wait_for "$applog" 3 'key: on' "$km"; then pass "a second click on the box made the panel key: key: on"; else
  flunk "no 'key: on' after the second click on the box; the test stops before posting keys"; exit 1
fi
"$drive" type hi
pm="$(lines "$applog")"
"$drive" click "$app" $((x0 + 120)) 251 >/dev/null 2>&1 || flunk "click the Folder row while typing"
if wait_for "$applog" 3 'key: off' "$pm"; then pass "a Folder row press while typing gave the keyboard back: key: off"; else flunk "no 'key: off' after the Folder row press while typing; the test stops"; exit 1; fi
sleep 0.6
if since "$applog" "$pm" | grep -qF 'key: on'; then flunk "the Folder row press that ended typing made the panel key again: key: on"; else pass "the press that ended typing left the panel not key"; fi
"$drive" type q
sleep 0.5
text3="$("$drive" axtext "$te" "$decoy_name" 2>&1)"
if [ "$text3" = "azq" ]; then pass "a key after the press that ended typing landed in TextEdit: the decoy reads azq"; else flunk "the decoy reads [$text3] after the press that ended typing, want azq"; fi

# A click on Back while typing: the give-back runs during the press, and Back acts on `click`, so the card closing
# shows the click still reached the page. Back spans 196..214 pt below the panel top, from x 14 of the panel. A
# collapse also leaves the list showing, so Back is judged only when the panel did not collapse after the click.
judged=0
for attempt in 1 2; do
  need_card "before the Back round"
  km="$(lines "$applog")"
  "$drive" click "$app" $((x0 + 120)) 333 >/dev/null 2>&1 || flunk "click the box for the Back round"
  if wait_for "$applog" 3 'key: on' "$km"; then pass "a click on the box for the Back round made the panel key: key: on"; else
    flunk "no 'key: on' after the click on the box for the Back round; the test stops before posting keys"; exit 1
  fi
  "$drive" type hi
  pm="$(lines "$applog")"
  "$drive" click "$app" $((x0 + 40)) 205 >/dev/null 2>&1 || flunk "click Back while typing"
  if wait_for "$applog" 3 'key: off' "$pm"; then pass "a click on Back while typing gave the keyboard back: key: off"; else flunk "no 'key: off' after the click on Back; the test stops"; exit 1; fi
  sleep 0.8
  if collapsed_since "$pm"; then echo "INFO Back not judged (try $attempt): the panel collapsed after the click"; continue; fi
  am="$(lines "$applog")"
  "$drive" click "$app" $((x0 + 120)) 229 >/dev/null 2>&1 || flunk "click row 1 after Back"
  if wait_for "$applog" 5 'page: detail sess-type found' "$am"; then pass "Back closed the card, so its click landed: row 1 opened it again"; else flunk "no 'page: detail sess-type found' after Back and a row 1 click: the click on Back did not close the card"; fi
  card_at="$(lines "$applog")"
  judged=1
  sleep 0.8
  break
done
[ "$judged" -eq 1 ] || flunk "Back was not judged: the panel collapsed after the click on both tries"
if since "$applog" "$pm" | grep -qF 'key: on'; then flunk "a click after Back made the panel key: key: on"; fi
"$drive" type w
sleep 0.5
text4="$("$drive" axtext "$te" "$decoy_name" 2>&1)"
if [ "$text4" = "azqw" ]; then pass "a key after Back and the row click landed in TextEdit: the decoy reads azqw"; else flunk "the decoy reads [$text4] after Back and the row click, want azqw"; fi

# A resign: another app becomes active while typing (Finder, then TextEdit again), and the page gives the keyboard back
# on the `key` event. A click on the Folder row afterwards must not make the panel key.
need_card "before the resign round"
km="$(lines "$applog")"
"$drive" click "$app" $((x0 + 120)) 333 >/dev/null 2>&1 || flunk "click the box a fourth time"
if wait_for "$applog" 3 'key: on' "$km"; then pass "a fourth click on the box made the panel key: key: on"; else
  flunk "no 'key: on' after the fourth click on the box; the test stops before posting keys"; exit 1
fi
"$drive" type hi
sm="$(lines "$applog")"
w0="$(lines "$watchlog")"
osascript -e 'tell application "Finder" to activate' >/dev/null && sleep 0.5 && osascript -e 'tell application "TextEdit" to activate' >/dev/null
if wait_for "$applog" 8 'key: off' "$sm"; then pass "the switch to Finder and back resigned the panel: key: off"; else flunk "no 'key: off' after the switch to Finder and back; the test stops"; exit 1; fi
sleep 0.8
w1="$(lines "$watchlog")"
front_is_textedit "after the switch back"
need_card "after the switch back"
cm="$(lines "$applog")"
"$drive" click "$app" $((x0 + 120)) 251 >/dev/null 2>&1 || flunk "click the Folder row after the resign"
sleep 0.6
if since "$applog" "$cm" | grep -qF 'key: '; then flunk "the Folder row click after the resign changed the keyboard: $(since "$applog" "$cm" | grep -F 'key: ' | tr '\n' ' ')"; fi
"$drive" type e
sleep 0.5
text5="$("$drive" axtext "$te" "$decoy_name" 2>&1)"
if [ "$text5" = "azqwe" ]; then pass "a key after the resign and a Folder row click landed in TextEdit: the decoy reads azqwe"; else flunk "the decoy reads [$text5] after the resign and a Folder row click, want azqwe"; fi

# The pointer leaves: the panel collapses as usual, so ordering it out and back in kept its hover tracking.
need_card "before the pointer leaves"
hm="$(lines "$applog")"
"$drive" move $((x0 + 210)) 640
if wait_for "$applog" 3 'window: pill' "$hm"; then pass "the pointer left and the panel collapsed: window: pill"; else flunk "no 'window: pill' within 3 s of the pointer leaving"; fi

if [ -s "$received" ]; then flunk "the fake inbox received [$(cat "$received")], want nothing"; else pass "nothing reached the fake inbox"; fi
others="$({ head -n "$w0" "$watchlog"; tail -n +$((w1 + 1)) "$watchlog"; } | grep '^activated' | grep -v ' com.apple.TextEdit$')"
switch="$(sed -n "$((w0 + 1)),${w1}p" "$watchlog" | grep '^activated' | grep -v ' com.apple.TextEdit$' | grep -v ' com.apple.finder$')"
if [ -n "$others$switch" ]; then flunk "an app other than TextEdit (and Finder during the switch) was activated: $(echo $others $switch)"; else pass "no app other than TextEdit was activated, and Finder only during the switch"; fi
echo "INFO collapses with the pointer on the panel: $collapses"
echo "INFO key: on lines: $(grep -c '^key: on' "$applog"), key: off lines: $(grep -c '^key: off' "$applog")"

if [ "$fail" -ne 0 ]; then echo "--- app.log"; cat "$applog"; echo "--- watch.log"; cat "$watchlog"; fi
exit "$fail"
