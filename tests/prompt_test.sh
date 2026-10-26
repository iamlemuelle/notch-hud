#!/bin/bash
# End to end, on the real panel and the real keyboard path: the prompt box takes the keyboard and gives it back, and
# Return sends (CONTRACT.md section 3 "Key focus" and section 11). The session's inbox is a fake: a Python AF_UNIX
# stream server in a temp dir that appends every byte it receives to a file (see tests/inbox_test.sh); no test here
# names a real Claude Code session's socket. TextEdit is frontmost with an empty document as a decoy. A real click opens
# a session's card and another clicks the box; posted keys `hello` and Return reach the page (the app logs
# `prompt: sent sess-type 5 chars` and the fake receives the framed line); Return ends typing, so the box is clicked
# again; the pointer leaves and the panel stays open; Command-V pastes a test string into the box, then two more
# letters; Command-H, -W and -Q leave the panel alone; Return sends (`prompt: sent sess-type 13 chars`), and the exact
# line at the fake, `pasted-textok`, shows the paste worked and the menu keys typed nothing; after the panel collapses,
# a new card and click start typing again and Esc gives the keyboard back (`key: off`) and the panel collapses; posted
# keys `xyz` then land in the decoy document, which shows focus came back, and nothing more reached the fake. TextEdit
# stays the frontmost app throughout and notch-hud is never activated.
#
# The paste puts the test string on the general pasteboard for half a second, marked transient and concealed so
# clipboard managers skip it, then puts back exactly what was there (Command-V reads only the general pasteboard).
#
# It posts mouse and key events, so the terminal running it needs Accessibility. Exit 0 when every check passes, 1 when
# one fails, 2 when the environment cannot run it, including a failure while someone used the trackpad, mouse or
# keyboard. Do not touch the mouse or the keyboard or switch apps while it runs.
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
app=""; watcher=""; hands=""; inbox_pid=""; cursor_x=""; cursor_y=""; te=""; te_started=0
drive="$tmp/drive"
decoy_name="notch-prompt-decoy-$$.txt"
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
  [ -n "$hands" ] && kill "$hands" 2>/dev/null
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
applog="$tmp/app.log"; watchlog="$tmp/watch.log"; inboxlog="$tmp/inbox.log"; handlog="$tmp/hands.log"
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
front_is_textedit() {
  local f; f="$("$drive" front)"
  if [ "${f#* }" = "com.apple.TextEdit" ]; then pass "frontmost app is TextEdit $1"; else flunk "frontmost app is [$f] $1, want TextEdit"; fi
}

start_fake_inbox

# The lines the fake must hold after the first send, and after the second.
printf '{"type":"user","message":{"role":"user","content":"hello"}}\n' >"$tmp/want1"
cp "$tmp/want1" "$tmp/want2"
printf '{"type":"user","message":{"role":"user","content":"pasted-textok"}}\n' >>"$tmp/want2"

# One session through the real hook, with the fake as its inbox: row 1 is "typeproj" (session sess-type).
printf '%s' '{"session_id":"sess-type","cwd":"/x/typeproj","hook_event_name":"PreToolUse","tool_name":"Read"}' | env -u __CFBundleIdentifier CLAUDE_CODE_MESSAGING_SOCKET="$sock" "$root/hooks/notch-hook.sh"

read -r cursor_x cursor_y < <("$drive" cursor)
"$drive" hands >"$handlog" 2>&1 & hands=$!
wait_for "$handlog" 5 watching || { echo "SKIP cannot watch for hardware input: $(cat "$handlog")"; exit 2; }

# The app first, since an unbundled binary comes up frontmost (wry activates it when it builds the webview); TextEdit
# then takes the front.
"$root/src-tauri/target/release/notch-hud" 2>"$applog" & app=$!
if ! wait_for "$applog" 15 'page: loaded'; then flunk "page loads"; echo "--- app.log"; cat "$applog"; finish 1; fi
if ! wait_for "$applog" 15 '"total":1'; then flunk "the app sees the session"; echo "--- app.log"; cat "$applog"; finish 1; fi

pgrep -xq TextEdit || te_started=1
: >"$decoy"
open -a TextEdit "$decoy" || { echo "SKIP cannot open TextEdit"; exit 2; }
for _ in $(seq 1 50); do
  f="$("$drive" front)"
  if [ "${f#* }" = "com.apple.TextEdit" ]; then te="${f%% *}"; "$drive" axtext "$te" "$decoy_name" >/dev/null 2>&1 && break; fi
  sleep 0.2
done
[ -n "$te" ] && text0="$("$drive" axtext "$te" "$decoy_name" 2>&1)" || { echo "SKIP TextEdit did not come up frontmost with the decoy"; exit 2; }
if [ "$text0" = "" ]; then pass "TextEdit is frontmost (pid $te) with the empty decoy $decoy_name"; else flunk "the decoy is not empty: [$text0]"; finish 1; fi

"$drive" watch >"$watchlog" 2>&1 & watcher=$!
wait_for "$watchlog" 5 watching || { echo "FAIL activation watcher did not start"; exit 2; }

x0_line="$("$drive" hover "$app" 2>&1)"; rc=$?
if [ "$rc" -eq 2 ]; then echo "SKIP $x0_line"; exit 2; fi
if [ "$rc" -ne 0 ]; then flunk "panel expands on hover"; echo "$x0_line"; finish 1; fi
x0="${x0_line#x0=}"
pass "panel expanded with its left edge at x=$x0"

# Row 1 spans 218..240 pt below the panel top (CONTRACT.md section 4).
am="$(lines "$applog")"
"$drive" click "$app" $((x0 + 120)) 229 >/dev/null 2>&1 || flunk "click row 1"
if wait_for "$applog" 5 'page: detail sess-type found' "$am"; then pass "a click on row 1 opened the card: page: detail sess-type found"; else flunk "no 'page: detail sess-type found' after the row click"; finish 1; fi
sleep 0.8

# The box: card top 196, header 18, 4 pt, four 22 pt rows, 10 pt, so 316..350 pt below the panel top.
km="$(lines "$applog")"
"$drive" click "$app" $((x0 + 120)) 333 >/dev/null 2>&1 || flunk "click the box"
if wait_for "$applog" 3 'key: on' "$km"; then pass "a click on the box made the panel key: key: on"; else
  # Without the keyboard, the keys below would go to whatever app is in front.
  flunk "no 'key: on' after the click on the box; the test stops before posting keys"; finish 1
fi
front_is_textedit "after the panel took the keyboard"

dm="$(lines "$applog")"
"$drive" type hello
"$drive" key return
reached=0
if wait_for "$applog" 3 'prompt: sent sess-type 5 chars' "$dm"; then pass "posted keys reached the box and Return sent: prompt: sent sess-type 5 chars"; else flunk "no 'prompt: sent sess-type 5 chars' after posting hello and Return; got: $(since "$applog" "$dm" | grep -F 'prompt:' | tr '\n' ' ')"; fi
if wait_received 3 "$tmp/want1"; then pass "the fake inbox received exactly the framed hello line"; reached=1; else flunk "the fake inbox received [$(cat "$received")], want the framed hello line"; fi
if wait_for "$applog" 3 'key: off' "$dm"; then pass "Return ended typing: key: off"; else flunk "no 'key: off' after Return"; fi
# From here on, keys are posted only when the keys above reached the box: otherwise they would go to TextEdit.
if [ "$reached" -ne 1 ]; then flunk "no more keys are posted: the keys above did not reach the box"; finish 1; fi

# Return ended typing, so the box is clicked again, and the test goes on only if the panel took the keyboard again.
km="$(lines "$applog")"
"$drive" click "$app" $((x0 + 120)) 333 >/dev/null 2>&1 || flunk "click the box again"
if wait_for "$applog" 3 'key: on' "$km"; then pass "a second click on the box made the panel key again: key: on"; else
  flunk "no 'key: on' after the second click on the box; the test stops before posting keys"; finish 1
fi
front_is_textedit "after the panel took the keyboard again"

# The pointer leaves the panel while typing: a real mouseMoved well below it. The panel stays open.
hm="$(lines "$applog")"
"$drive" move $((x0 + 210)) 640
if wait_for "$applog" 3 'hover: out' "$hm"; then pass "the pointer left the panel: hover: out"; else flunk "no 'hover: out' after moving the pointer away"; fi
sleep 1
if since "$applog" "$hm" | grep -qF 'window: pill'; then flunk "the panel collapsed on hover-out while typing"; else pass "no collapse 1 s after hover-out while typing"; fi
b="$("$drive" bounds "$app")"
case "$b" in *" 452 396") pass "window is still the open window: $b";; *) flunk "window bounds are [$b] after hover-out while typing, want w=452 h=396";; esac

# The draft is empty again after the send: pasted-text (11) + 2 more letters = 13 chars. Nothing is sent until Return.
paste_out="$("$drive" paste "pasted-text" 2>&1)"; rc=$?
if [ "$rc" -eq 0 ]; then pass "drive paste: $paste_out"; else flunk "drive paste: $paste_out"; fi
"$drive" type ok
front_is_textedit "while typing"

# Menu key equivalents reach notch-hud while its panel is key: Command-H, Command-W and Command-Q must not hide,
# close or quit it (CONTRACT.md section 11, "Menu"), and must type nothing. Command-Q only when this test started
# TextEdit, since it would quit a TextEdit that was already running. Each key goes only while the panel still has the
# keyboard and TextEdit is still in front, and Command-H, which would put another app in front, goes last. Once the
# keyboard is not the panel's, the test posts no more keys and stops: they would land in whatever app is in front.
not_ours() {
  since "$applog" "$km" | grep -qF 'key: off' && { echo "the panel gave up the keyboard (key: off)"; return 0; }
  local f; f="$("$drive" front)"
  [ "${f#* }" = "com.apple.TextEdit" ] || { echo "TextEdit is no longer in front ($f)"; return 0; }
  return 1
}
if why="$(not_ours)"; then flunk "menu keys not tried and the test stops: $why"; finish 1; fi
cm="$(lines "$applog")"
keys="w"
if [ "$te_started" -eq 1 ]; then keys="$keys q"; else echo "INFO Command-Q skipped: TextEdit was already running"; fi
for k in $keys h; do
  if why="$(not_ours)"; then flunk "stopped before Command-$k: $why"; finish 1; fi
  "$drive" cmd "$k"
done
sleep 0.5
if kill -0 "$app" 2>/dev/null; then pass "notch-hud is still running after Command-$(echo $keys h | tr ' ' '/')"; else flunk "notch-hud quit on a menu key while typing"; app=""; finish 1; fi
if why="$(not_ours)"; then flunk "stopped after the menu keys: $why"; finish 1; fi
b="$("$drive" bounds "$app")"
case "$b" in *" 452 396") pass "the panel is still on screen and open after the menu keys: $b";; *) flunk "the panel is [$b] after the menu keys, want w=452 h=396";; esac
"$drive" key return
if wait_for "$applog" 3 'prompt: sent sess-type 13 chars' "$cm"; then pass "Return sent the pasted draft: prompt: sent sess-type 13 chars"; else flunk "no 'prompt: sent sess-type 13 chars' after the menu keys and Return; got: $(since "$applog" "$cm" | grep -F 'prompt:' | tr '\n' ' ')"; fi
if wait_received 3 "$tmp/want2"; then pass "Command-V pasted into the box and the menu keys typed nothing: the fake received exactly pasted-textok"; else flunk "the fake inbox holds [$(cat "$received")], want the hello line then the pasted-textok line"; fi
front_is_textedit "after the second send"
# With the pointer gone, ending typing by the send collapses the panel as usual.
if wait_for "$applog" 3 'window: pill' "$cm"; then pass "with the pointer gone the panel collapsed after the send: window: pill"; else flunk "no 'window: pill' within 3 s of the send with the pointer away"; fi

# Typing again, to end it with Esc: the pointer back on the pill expands the panel, which starts on the list.
x0_line="$("$drive" hover "$app" 2>&1)"; rc=$?
if [ "$rc" -ne 0 ]; then flunk "panel expands on hover the second time"; echo "$x0_line"; finish 1; fi
x0="${x0_line#x0=}"
am="$(lines "$applog")"
"$drive" click "$app" $((x0 + 120)) 229 >/dev/null 2>&1 || flunk "click row 1 again"
if wait_for "$applog" 5 'page: detail sess-type found' "$am"; then pass "row 1 opened the card again: page: detail sess-type found"; else flunk "no 'page: detail sess-type found' after the second row click; the test stops"; finish 1; fi
sleep 0.8
km="$(lines "$applog")"
"$drive" click "$app" $((x0 + 120)) 333 >/dev/null 2>&1 || flunk "click the box a third time"
if wait_for "$applog" 3 'key: on' "$km"; then pass "a third click on the box made the panel key: key: on"; else
  flunk "no 'key: on' after the third click on the box; the test stops before posting Esc and keys"; finish 1
fi
front_is_textedit "after the panel took the keyboard a third time"
"$drive" move $((x0 + 210)) 640
if ! wait_for "$applog" 3 'hover: out' "$km"; then flunk "no 'hover: out' after moving the pointer away"; fi
sleep 0.5

em="$(lines "$applog")"
"$drive" key escape
if wait_for "$applog" 3 'key: off' "$em"; then pass "Esc gave the keyboard back: key: off"; else flunk "no 'key: off' after Esc"; fi
if wait_for "$applog" 3 'window: pill' "$em"; then pass "with the pointer gone the panel collapsed: window: pill"; else flunk "no 'window: pill' within 3 s of Esc with the pointer away"; fi
sleep 0.3

"$drive" type xyz
sleep 0.5
text1="$("$drive" axtext "$te" "$decoy_name" 2>&1)"
if [ "$text1" = "xyz" ]; then pass "keys after Esc landed in TextEdit: the decoy reads xyz"; else flunk "the decoy reads [$text1], want xyz (keyboard focus did not come back, or keys leaked into it while typing)"; fi
front_is_textedit "after the give-back"
if cmp -s "$received" "$tmp/want2"; then pass "Esc and the keys after it sent nothing: the fake still holds exactly the two lines"; else flunk "the fake inbox holds [$(cat "$received")] at the end, want exactly the two sent lines"; fi

if grep '^activated' "$watchlog" | grep -qv ' com.apple.TextEdit$'; then flunk "an app other than TextEdit was activated"; grep '^activated' "$watchlog"; else pass "no app other than TextEdit was activated"; fi
grep -q '^activated' "$watchlog" && echo "INFO TextEdit activations: $(grep -c '^activated' "$watchlog")"
echo "INFO key: on lines: $(grep -c '^key: on' "$applog"), key: off lines: $(grep -c '^key: off' "$applog")"

if [ "$fail" -ne 0 ]; then echo "--- app.log"; cat "$applog"; echo "--- watch.log"; cat "$watchlog"; fi
finish "$fail"
