#!/bin/bash
# End to end, on the real panel: hover follows the pointer, not the keyboard (CONTRACT.md sections 3 and 8). With the
# pointer resting on a session's card, the prompt box takes the keyboard and gives it back many times, by Esc, by a
# press on the card's Folder row and by another app becoming active, and the app logs no `hover: out` and no
# `window: pill` until the pointer leaves; then the panel collapses as usual. TextEdit is frontmost with an empty
# document as a decoy, so a key that misses the box lands there instead of in another app.
#
# It posts mouse and key events, so the terminal running it needs Accessibility. Exit 0 when every check passes, 1 when
# one fails, 2 when the environment cannot run it, including a failure while someone used the trackpad, mouse or
# keyboard: a real pointer leaving the card is a real hover out. Do not touch the mouse or the keyboard or switch apps
# while it runs.
# It runs its own app process with a temp NOTCH_DIR and kills only that process; it never touches the installed app. It
# opens its decoy in TextEdit, closes that document at the end, and quits TextEdit only if it was not running before.
set -u
unset CLAUDE_CODE_MESSAGING_SOCKET CLAUDE_CODE_MESSAGING_TOKEN  # never let a test reach a real session's inbox
root="$(cd "$(dirname "$0")/.." && pwd)"
export PATH="/opt/homebrew/opt/rustup/bin:$PATH"
fail=0
pass() { echo "PASS $1"; }
flunk() { echo "FAIL $1"; fail=1; }

rounds="${HOVER_KEY_ROUNDS:-4}"
tmp="$(mktemp -d)"
app=""; hands=""; cursor_x=""; cursor_y=""; te=""; te_started=0
drive="$tmp/drive"
decoy_name="notch-hover-key-decoy-$$.txt"
decoy="$tmp/$decoy_name"
cleanup() {
  [ -n "$cursor_x" ] && "$drive" warp "$cursor_x" "$cursor_y" 2>/dev/null
  if [ -n "$te" ]; then
    "$drive" axclose "$te" "$decoy_name" 2>/dev/null
    [ "$te_started" -eq 1 ] && "$drive" quit "$te" 2>/dev/null
  fi
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
applog="$tmp/app.log"; handlog="$tmp/hands.log"
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

# One session through the real hook: row 1 is "hoverproj" (session sess-hover). Its inbox is a path nothing listens
# on, so the box takes typing; Return is never pressed, so nothing is sent.
printf '%s' '{"session_id":"sess-hover","cwd":"/x/hoverproj","hook_event_name":"PreToolUse","tool_name":"Read"}' | env -u __CFBundleIdentifier CLAUDE_CODE_MESSAGING_SOCKET="$tmp/inbox.sock" "$root/hooks/notch-hook.sh"

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
[ -n "$te" ] && "$drive" axtext "$te" "$decoy_name" >/dev/null 2>&1 || { echo "SKIP TextEdit did not come up frontmost with the decoy"; exit 2; }
pass "TextEdit is frontmost (pid $te) with the decoy $decoy_name"

x0_line="$("$drive" hover "$app" 2>&1)"; rc=$?
if [ "$rc" -eq 2 ]; then echo "SKIP $x0_line"; exit 2; fi
if [ "$rc" -ne 0 ]; then flunk "panel expands on hover"; echo "$x0_line"; finish 1; fi
x0="${x0_line#x0=}"
pass "panel expanded with its left edge at x=$x0"

# Row 1 spans 218..240 pt below the panel top (CONTRACT.md section 4).
am="$(lines "$applog")"
"$drive" click "$app" $((x0 + 120)) 229 >/dev/null 2>&1 || flunk "click row 1"
if wait_for "$applog" 5 'page: detail sess-hover found' "$am"; then pass "a click on row 1 opened the card"; else flunk "no 'page: detail sess-hover found' after the row click"; echo "--- app.log"; cat "$applog"; finish 1; fi
sleep 0.8
card_at="$(lines "$applog")"

# Starts typing: a click on the box, 316..350 pt below the panel top (card top 196, header 18, 4 pt, four 22 pt rows,
# 10 pt). Keys are posted only once the panel logged `key: on`; a panel made key by the click before `focus` ran logs
# none, so a missing line skips the keys but still ends typing.
start_typing() {
  local km out; km="$(lines "$applog")"
  out="$("$drive" click "$app" $((x0 + 120)) 333 2>&1)" || flunk "click the box ($1): $out"
  if wait_for "$applog" 2 'key: on' "$km"; then "$drive" type hi; else sleep 0.3; fi
}
# Fails when anything after the card opened shows hover leaving or the panel collapsing.
still_open() {
  local bad; bad="$(since "$applog" "$card_at" | grep -E '^(hover: out|window: pill)' | tr '\n' ' ')"
  if [ -z "$bad" ]; then pass "$1: the pointer is on the card and the panel stayed open"; return 0; fi
  flunk "$1 with the pointer on the card logged: $bad"
  echo "--- app.log since the card opened"; since "$applog" "$card_at"
  finish 1
}

for r in $(seq 1 "$rounds"); do
  start_typing "Esc round $r"
  em="$(lines "$applog")"
  "$drive" key escape
  wait_for "$applog" 3 'key: off' "$em" || flunk "no 'key: off' after Esc (round $r)"
  sleep 0.6
  still_open "Esc ended typing (round $r)"

  # The Folder row, 240..262 pt below the panel top: a press outside the box ends typing during that press.
  start_typing "row press round $r"
  pm="$(lines "$applog")"
  out="$("$drive" click "$app" $((x0 + 120)) 251 2>&1)" || flunk "click the Folder row (round $r): $out"
  wait_for "$applog" 3 'key: off' "$pm" || flunk "no 'key: off' after the Folder row press (round $r)"
  sleep 0.6
  still_open "a Folder row press ended typing (round $r)"

  # A resign: another app becomes active while typing, and the page gives the keyboard back on the `key` event.
  start_typing "resign round $r"
  sm="$(lines "$applog")"
  osascript -e 'tell application "Finder" to activate' >/dev/null && sleep 0.5 && osascript -e 'tell application "TextEdit" to activate' >/dev/null
  wait_for "$applog" 8 'key: off' "$sm" || flunk "no 'key: off' after the switch to Finder and back (round $r)"
  sleep 0.8
  still_open "a switch to Finder and back ended typing (round $r)"
done

hm="$(lines "$applog")"
"$drive" move $((x0 + 210)) 640
if wait_for "$applog" 3 'window: pill' "$hm"; then pass "the pointer left and the panel collapsed: window: pill"; else flunk "no 'window: pill' within 3 s of the pointer leaving"; fi

if [ "$fail" -ne 0 ]; then echo "--- app.log"; cat "$applog"; fi
finish "$fail"
