#!/bin/bash
# End to end, on the real panel and the real keyboard path: the prompt box takes the keyboard and gives it back
# (CONTRACT.md section 3 "Key focus" and section 11). TextEdit is frontmost with an empty document as a decoy. A real
# click opens a session's card and another clicks the box; posted keys `hello` and Return reach the page (the app logs
# `prompt: draft 5 chars`); the pointer leaves and the panel stays open; Command-V pastes a test string into the box,
# then two more letters and Return (`prompt: draft 18 chars`); Command-H, -W and -Q leave the panel alone; Esc gives
# the keyboard back (`key: off`) and the panel collapses; posted keys `xyz` then land in the decoy document, which
# shows focus came back. TextEdit stays the frontmost app throughout and notch-hud is never activated.
#
# The paste puts the test string on the general pasteboard for half a second, marked transient and concealed so
# clipboard managers skip it, then puts back exactly what was there (Command-V reads only the general pasteboard).
#
# It posts mouse and key events, so the terminal running it needs Accessibility. Exit 0 when every check passes, 1 when
# one fails, 2 when the environment cannot run it. Do not touch the mouse or the keyboard or switch apps while it runs.
# It runs its own app process with a temp NOTCH_DIR and kills only that process; it never touches the installed app. It
# opens its decoy in TextEdit, closes that document at the end, and quits TextEdit only if it was not running before.
set -u
root="$(cd "$(dirname "$0")/.." && pwd)"
export PATH="/opt/homebrew/opt/rustup/bin:$PATH"
fail=0
pass() { echo "PASS $1"; }
flunk() { echo "FAIL $1"; fail=1; }

tmp="$(mktemp -d)"
app=""; watcher=""; cursor_x=""; cursor_y=""; te=""; te_started=0
drive="$tmp/drive"
decoy_name="notch-prompt-decoy-$$.txt"
decoy="$tmp/$decoy_name"
cleanup() {
  [ -n "$cursor_x" ] && "$drive" warp "$cursor_x" "$cursor_y" 2>/dev/null
  if [ -n "$te" ]; then
    "$drive" axclose "$te" "$decoy_name" 2>/dev/null
    [ "$te_started" -eq 1 ] && "$drive" quit "$te" 2>/dev/null
  fi
  [ -n "$watcher" ] && kill "$watcher" 2>/dev/null
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
applog="$tmp/app.log"; watchlog="$tmp/watch.log"
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

# One session through the real hook: row 1 is "typeproj" (session sess-type).
printf '%s' '{"session_id":"sess-type","cwd":"/x/typeproj","hook_event_name":"PreToolUse","tool_name":"Read"}' | "$root/hooks/notch-hook.sh"

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
if wait_for "$applog" 3 'key: on' "$km"; then pass "a click on the box made the panel key: key: on"; else flunk "no 'key: on' after the click on the box"; fi
front_is_textedit "after the panel took the keyboard"

dm="$(lines "$applog")"
"$drive" type hello
"$drive" key return
if wait_for "$applog" 3 'prompt: draft 5 chars' "$dm"; then pass "posted keys reached the box: prompt: draft 5 chars"; else flunk "no 'prompt: draft 5 chars' after posting hello and Return"; fi

# The pointer leaves the panel while typing: a real mouseMoved well below it. The panel stays open.
hm="$(lines "$applog")"
"$drive" move $((x0 + 210)) 640
if wait_for "$applog" 3 'hover: out' "$hm"; then pass "the pointer left the panel: hover: out"; else flunk "no 'hover: out' after moving the pointer away"; fi
sleep 1
if since "$applog" "$hm" | grep -qF 'window: pill'; then flunk "the panel collapsed on hover-out while typing"; else pass "no collapse 1 s after hover-out while typing"; fi
b="$("$drive" bounds "$app")"
case "$b" in *" 452 396") pass "window is still the open window: $b";; *) flunk "window bounds are [$b] after hover-out while typing, want w=452 h=396";; esac

# hello (5) + pasted-text (11) + 2 more letters = 18; Return then traces the whole draft.
pm="$(lines "$applog")"
paste_out="$("$drive" paste "pasted-text" 2>&1)"; rc=$?
if [ "$rc" -eq 0 ]; then pass "drive paste: $paste_out"; else flunk "drive paste: $paste_out"; fi
"$drive" type ok
"$drive" key return
if wait_for "$applog" 3 'prompt: draft 18 chars' "$pm"; then pass "Command-V pasted into the box: prompt: draft 18 chars"; else flunk "no 'prompt: draft 18 chars' after Command-V, ok and Return; got: $(since "$applog" "$pm" | grep -F 'prompt:' | tr '\n' ' ')"; fi
front_is_textedit "while typing"

# Menu key equivalents reach notch-hud while its panel is key: Command-H, Command-W and Command-Q must not hide,
# close or quit it (CONTRACT.md section 11, "Menu"), and must type nothing. Only when the keys above reached the box,
# since otherwise they would go to TextEdit; Command-Q only when this test started TextEdit, for the same reason. Each
# key goes only while the panel still has the keyboard and TextEdit is still in front, and Command-H, which would put
# another app in front, goes last.
still_ours() { ! since "$applog" "$km" | grep -qF 'key: off' && [ "$("$drive" front | cut -d' ' -f2)" = "com.apple.TextEdit" ]; }
if since "$applog" "$pm" | grep -qF 'prompt: draft 18 chars' && still_ours; then
  cm="$(lines "$applog")"
  keys="w"
  if [ "$te_started" -eq 1 ]; then keys="$keys q"; else echo "INFO Command-Q skipped: TextEdit was already running"; fi
  for k in $keys h; do
    still_ours || { flunk "the panel lost the keyboard before Command-$k; the menu keys stopped there"; break; }
    "$drive" cmd "$k"
  done
  sleep 0.5
  if kill -0 "$app" 2>/dev/null; then pass "notch-hud is still running after the menu keys"; else flunk "notch-hud quit on a menu key while typing"; app=""; exit 1; fi
  b="$("$drive" bounds "$app")"
  case "$b" in *" 452 396") pass "the panel is still on screen and open after Command-H and Command-W: $b";; *) flunk "the panel is [$b] after Command-H and Command-W, want w=452 h=396";; esac
  "$drive" key return
  if wait_for "$applog" 3 'prompt: draft 18 chars' "$cm"; then pass "the menu keys typed nothing: prompt: draft 18 chars again"; else flunk "no 'prompt: draft 18 chars' after the menu keys; got: $(since "$applog" "$cm" | grep -F 'prompt:' | tr '\n' ' ')"; fi
else
  flunk "menu keys not tried: the keys above did not reach the box"
fi

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

if grep '^activated' "$watchlog" | grep -qv ' com.apple.TextEdit$'; then flunk "an app other than TextEdit was activated"; grep '^activated' "$watchlog"; else pass "no app other than TextEdit was activated"; fi
grep -q '^activated' "$watchlog" && echo "INFO TextEdit activations: $(grep -c '^activated' "$watchlog")"
echo "INFO key: on lines: $(grep -c '^key: on' "$applog"), key: off lines: $(grep -c '^key: off' "$applog")"

if [ "$fail" -ne 0 ]; then echo "--- app.log"; cat "$applog"; echo "--- watch.log"; cat "$watchlog"; fi
exit "$fail"
