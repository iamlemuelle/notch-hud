#!/bin/bash
# End to end, on the real panel and the real keyboard path: Return in the prompt box sends the text to the session's
# inbox socket (CONTRACT.md section 11, "Sending"). The inbox is a fake: a Python AF_UNIX stream server in a temp dir
# that appends every byte it receives to a file. The sessions are seeded through the real hook with that socket as their
# inbox; no test here names, reads or writes a real Claude Code session's socket.
#
# TextEdit is frontmost with an empty document as a decoy, so a stray key lands there. A real click opens the first
# session's card and another clicks the box (the test stops before posting any key if the panel did not take the
# keyboard: `key: on` must follow the click); posted keys `hello` and Return send. It passes when the app logs
# `prompt: sent sess-sent 5 chars`, the fake received exactly
# {"type":"user","message":{"role":"user","content":"hello"}} plus one newline and nothing else, and `key: off`
# follows (sending ended typing). A second session whose inbox is a regular file in the temp dir then gets text typed
# and Return: the app logs `prompt: rejected` and the fake receives nothing more. TextEdit stays the frontmost app
# throughout and notch-hud is never activated.
#
# It posts mouse and key events, so the terminal running it needs Accessibility. Exit 0 when every check passes, 1 when
# one fails, 2 when the environment cannot run it. Do not touch the mouse or the keyboard or switch apps while it runs.
# It runs its own app process with a temp NOTCH_DIR and kills only its own processes; it never touches the installed
# app. It opens its decoy in TextEdit, closes that document at the end, and quits TextEdit only if it was not running
# before.
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
decoy_name="notch-inbox-decoy-$$.txt"
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

# A sockaddr_un path holds 103 bytes and a NUL.
if [ "${#sock}" -ge 104 ]; then echo "SKIP the fake inbox path is ${#sock} bytes, the limit is 103: $sock"; exit 2; fi

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
# Waits up to $1 seconds for the fake's file to hold exactly the bytes of file $2.
wait_received() {
  local i=0
  while [ "$i" -lt $(($1 * 5)) ]; do
    cmp -s "$received" "$2" && return 0
    sleep 0.2; i=$((i + 1))
  done
  return 1
}

# The fake inbox: accepts connections one at a time and appends every byte it gets to $received.
cat >"$tmp/fake_inbox.py" <<'PY'
import socket, sys
path, out = sys.argv[1], sys.argv[2]
srv = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
srv.bind(path)
srv.listen(8)
open(out, "ab").close()
print("listening", flush=True)
while True:
    conn, _ = srv.accept()
    with conn:
        while True:
            data = conn.recv(65536)
            if not data:
                break
            with open(out, "ab") as f:
                f.write(data)
PY
python3 -I "$tmp/fake_inbox.py" "$sock" "$received" >"$inboxlog" 2>&1 & inbox_pid=$!
wait_for "$inboxlog" 5 listening || { echo "FAIL the fake inbox did not start"; cat "$inboxlog"; exit 2; }

# What the fake must have received after the first send: the framed line of CONTRACT.md section 11.
printf '{"type":"user","message":{"role":"user","content":"hello"}}\n' >"$tmp/want"

# Two sessions through the real hook, each with its own inbox. Section 2's sort puts NeedsPermission before Working, so
# row 1 is sess-rej (its inbox is a regular file, not a socket) and row 2 is sess-sent (the fake's socket).
printf 'not a socket' >"$tmp/notasocket"
hook() { printf '%s' "$2" | env -u __CFBundleIdentifier CLAUDE_CODE_MESSAGING_SOCKET="$1" "$root/hooks/notch-hook.sh"; }
hook "$sock" '{"session_id":"sess-sent","cwd":"/x/sentproj","hook_event_name":"PreToolUse","tool_name":"Read"}'
hook "$tmp/notasocket" '{"session_id":"sess-rej","cwd":"/x/rejproj","hook_event_name":"PermissionRequest","tool_name":"Bash"}'

read -r cursor_x cursor_y < <("$drive" cursor)

# The app first, since an unbundled binary comes up frontmost; TextEdit then takes the front.
"$root/src-tauri/target/release/notch-hud" 2>"$applog" & app=$!
if ! wait_for "$applog" 15 'page: loaded'; then flunk "page loads"; echo "--- app.log"; cat "$applog"; exit 1; fi
if ! wait_for "$applog" 15 '"total":2'; then flunk "the app sees both sessions"; echo "--- app.log"; cat "$applog"; exit 1; fi
sess_line="$(grep -F 'sessions: ' "$applog" | grep -F '"total":2' | tail -n 1)"
order="$(printf '%s' "$sess_line" | grep -o '"id":"sess-[a-z]*"' | tr '\n' ' ')"
if [ "$order" = '"id":"sess-rej" "id":"sess-sent" ' ]; then pass "rows are ordered sess-rej, sess-sent"; else flunk "row order is [$order], want sess-rej then sess-sent"; echo "$sess_line"; exit 1; fi

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

# Row 2 spans 240..262 pt below the panel top (CONTRACT.md section 4).
am="$(lines "$applog")"
"$drive" click "$app" $((x0 + 120)) 251 >/dev/null 2>&1 || flunk "click row 2"
if wait_for "$applog" 5 'page: detail sess-sent found' "$am"; then pass "a click on row 2 opened the card: page: detail sess-sent found"; else flunk "no 'page: detail sess-sent found' after the row click"; exit 1; fi
sleep 0.8

# The box: 316..350 pt below the panel top (section 11).
km="$(lines "$applog")"
"$drive" click "$app" $((x0 + 120)) 333 >/dev/null 2>&1 || flunk "click the box"
if wait_for "$applog" 3 'key: on' "$km"; then pass "a click on the box made the panel key: key: on"; else
  # Without the keyboard, the keys below would go to whatever app is in front, Return included.
  flunk "no 'key: on' after the click on the box; the test stops before posting keys"; exit 1
fi
front_is_textedit "after the panel took the keyboard"

sm="$(lines "$applog")"
"$drive" type hello
"$drive" key return
if wait_for "$applog" 3 'prompt: sent sess-sent 5 chars' "$sm"; then pass "Return sent the draft: prompt: sent sess-sent 5 chars"; else flunk "no 'prompt: sent sess-sent 5 chars' after posting hello and Return; got: $(since "$applog" "$sm" | grep -F 'prompt:' | tr '\n' ' ')"; fi
if wait_received 3 "$tmp/want"; then pass "the fake inbox received exactly the framed line"; else
  flunk "the fake inbox received [$(cat "$received")], want the framed hello line and one newline"
fi
if wait_for "$applog" 3 'key: off' "$sm"; then pass "sending ended typing: key: off"; else flunk "no 'key: off' after Return"; fi
sleep 0.5
if cmp -s "$received" "$tmp/want"; then pass "nothing more reached the fake inbox"; else flunk "the fake inbox holds [$(cat "$received")] half a second later"; fi
front_is_textedit "after the send"

# Second session: row 1, whose inbox is a regular file. Back to the list first (the back button, 205 pt below the panel
# top), then row 1 (218..240 pt).
bm="$(lines "$applog")"
"$drive" click "$app" $((x0 + 40)) 205 >/dev/null 2>&1 || flunk "click back"
sleep 0.8
"$drive" click "$app" $((x0 + 120)) 229 >/dev/null 2>&1 || flunk "click row 1"
if wait_for "$applog" 5 'page: detail sess-rej found' "$bm"; then pass "row 1 opened the card: page: detail sess-rej found"; else flunk "no 'page: detail sess-rej found' after going back and clicking row 1; the test stops"; exit 1; fi
sleep 0.8

km2="$(lines "$applog")"
"$drive" click "$app" $((x0 + 120)) 333 >/dev/null 2>&1 || flunk "click the box of the second session"
if wait_for "$applog" 3 'key: on' "$km2"; then pass "a click on the second box made the panel key: key: on"; else
  flunk "no 'key: on' after the click on the second box; the test stops before posting keys"; exit 1
fi
front_is_textedit "after the panel took the keyboard again"

rm_="$(lines "$applog")"
"$drive" type nope
"$drive" key return
if wait_for "$applog" 3 'prompt: rejected' "$rm_"; then pass "a regular file as inbox was refused: prompt: rejected"; else flunk "no 'prompt: rejected' after Return; got: $(since "$applog" "$rm_" | grep -F 'prompt:' | tr '\n' ' ')"; fi
if since "$applog" "$rm_" | grep -qF 'prompt: sent'; then flunk "the second session's text was reported sent"; fi
sleep 0.5
if cmp -s "$received" "$tmp/want"; then pass "the fake inbox received nothing more"; else flunk "the fake inbox holds [$(cat "$received")] after the rejected send"; fi
text1="$("$drive" axtext "$te" "$decoy_name" 2>&1)"
if [ "$text1" = "" ]; then pass "no key leaked into the decoy"; else flunk "the decoy reads [$text1], want empty"; fi
front_is_textedit "at the end"

if grep '^activated' "$watchlog" | grep -qv ' com.apple.TextEdit$'; then flunk "an app other than TextEdit was activated"; grep '^activated' "$watchlog"; else pass "no app other than TextEdit was activated"; fi

if [ "$fail" -ne 0 ]; then echo "--- app.log"; cat "$applog"; echo "--- watch.log"; cat "$watchlog"; echo "--- received"; cat "$received"; fi
exit "$fail"
