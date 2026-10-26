#!/bin/bash
# End to end, on the real panel: a real click on the second session row opens its detail card (CONTRACT.md section 3
# "Session detail" and section 4 "Detail card"). The click reaches the page (the app logs `page: detail <id> found` for row
# 2's session), the frontmost app does not change, nothing is activated, and the window keeps its size, 452 x 396
# (section 8, "Detail"), so showing the card never resizes it.
#
# It posts mouse events, so the terminal running it needs Accessibility. Exit 0 when every check passes, 1 when one
# fails, 2 when the environment cannot run it. Do not move the mouse or switch apps while it runs. It runs its own app
# process with a temp NOTCH_DIR and kills only that process; it never touches the installed app.
set -u
unset CLAUDE_CODE_MESSAGING_SOCKET CLAUDE_CODE_MESSAGING_TOKEN  # never let a test reach a real session's inbox
root="$(cd "$(dirname "$0")/.." && pwd)"
export PATH="/opt/homebrew/opt/rustup/bin:$PATH"
fail=0
pass() { echo "PASS $1"; }
flunk() { echo "FAIL $1"; fail=1; }

tmp="$(mktemp -d)"
app=""; watcher=""; cursor_x=""; cursor_y=""
drive="$tmp/drive"
cleanup() {
  [ -n "$cursor_x" ] && "$drive" warp "$cursor_x" "$cursor_y" 2>/dev/null
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

# Two sessions through the real hook. Section 2's sort puts NeedsPermission (priority 0) before Working (3), so row 1
# is "permproj" (session sess-perm) and row 2 is "workproj" (session sess-work).
hook() { printf '%s' "$1" | "$root/hooks/notch-hook.sh"; }
hook '{"session_id":"sess-work","cwd":"/x/workproj","hook_event_name":"PreToolUse","tool_name":"Read"}'
hook '{"session_id":"sess-perm","cwd":"/x/permproj","hook_event_name":"PermissionRequest","tool_name":"Bash"}'

read -r cursor_x cursor_y < <("$drive" cursor)

"$root/src-tauri/target/release/notch-hud" 2>"$applog" & app=$!

if ! wait_for "$applog" 15 'page: loaded'; then flunk "page loads"; echo "--- app.log"; cat "$applog"; exit 1; fi
if ! wait_for "$applog" 15 '"total":2'; then flunk "the app sees both sessions"; echo "--- app.log"; cat "$applog"; exit 1; fi
sess_line="$(grep -F 'sessions: ' "$applog" | grep -F '"total":2' | tail -n 1)"
order="$(printf '%s' "$sess_line" | grep -o '"id":"sess-[a-z]*"' | tr '\n' ' ')"
if [ "$order" = '"id":"sess-perm" "id":"sess-work" ' ]; then pass "rows are ordered sess-perm, sess-work"; else flunk "row order is [$order], want sess-perm then sess-work"; echo "$sess_line"; exit 1; fi

"$drive" watch >"$watchlog" 2>&1 & watcher=$!
wait_for "$watchlog" 5 watching || { echo "FAIL activation watcher did not start"; exit 2; }
front0="$("$drive" front)"

# `drive hover` waits for the open window (452 x 396 by default) and prints the panel's left edge as x0.
x0_line="$("$drive" hover "$app" 2>&1)"; rc=$?
if [ "$rc" -eq 2 ]; then echo "SKIP $x0_line"; exit 2; fi
if [ "$rc" -ne 0 ]; then flunk "panel expands on hover"; echo "$x0_line"; exit 1; fi
x0="${x0_line#x0=}"
pass "window expanded to 452 x 396 with the panel's left edge at x=$x0"

before="$("$drive" bounds "$app")"
case "$before" in *" 452 396") pass "window bounds before the click: $before";; *) flunk "window bounds before the click are [$before], want w=452 h=396";; esac

# Row 2: the session rows start 196 + 18 + 4 = 218 pt below the panel top, 22 pt each, so row 2 spans 240..262.
y_row2=251; x_row2=$((x0 + 120))
am="$(lines "$applog")"
out="$("$drive" click "$app" "$x_row2" "$y_row2" 2>&1)"; rc=$?
if [ "$rc" -eq 2 ]; then echo "SKIP $out"; exit 2; fi
[ "$rc" -ne 0 ] && { flunk "click row 2"; echo "$out"; }

if wait_for "$applog" 5 'page: detail sess-work found' "$am"; then pass "the app logged: page: detail sess-work found"; else flunk "the app did not log: page: detail sess-work found"; fi
if tail -n +$((am + 1)) "$applog" | grep -qF 'page: detail sess-perm'; then flunk "the click opened row 1 (sess-perm)"; fi
if tail -n +$((am + 1)) "$applog" | grep -qF 'page: detail rejected'; then flunk "the page sent an id the app rejected"; fi
sleep 0.8

# A new record that keeps sess-work Working with another tool: the summary does not change, so only the records event
# can make the open card ask again (CONTRACT.md section 3 and section 4). Expect a second `found` line within 5 s.
dm="$(lines "$applog")"
hook '{"session_id":"sess-work","cwd":"/x/workproj","hook_event_name":"PostToolUse","tool_name":"Grep"}'
if wait_for "$applog" 5 'page: detail sess-work found' "$dm"; then pass "a new record refetched the open card: page: detail sess-work found again"; else flunk "no second 'page: detail sess-work found' within 5 s of a new record"; fi

now="$("$drive" front)"
if [ "$now" = "$front0" ]; then pass "frontmost app unchanged ($front0)"; else flunk "frontmost app changed from [$front0] to [$now]"; fi
if grep -q '^activated' "$watchlog"; then flunk "no app activation while driving"; grep '^activated' "$watchlog"; else pass "no app activation while driving"; fi
after="$("$drive" bounds "$app")"
if [ "$after" = "$before" ]; then pass "window bounds unchanged by the click: $after"; else flunk "window bounds changed from [$before] to [$after]"; fi
case "$after" in *" 452 396") pass "window is 452 x 396 with the card shown";; *) flunk "window bounds after the click are [$after], want w=452 h=396";; esac

if [ "$fail" -ne 0 ]; then echo "--- app.log"; cat "$applog"; echo "--- watch.log"; cat "$watchlog"; fi
exit "$fail"
