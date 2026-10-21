#!/bin/bash
# Behavior tests for hooks/notch-hook.sh, written against CONTRACT.md section 1.
HERE="$(cd "$(dirname "$0")" && pwd)"
HOOK="$HERE/../hooks/notch-hook.sh"
ROOT="$(mktemp -d)"
trap 'chmod -R u+rwx "$ROOT" 2>/dev/null; rm -rf "$ROOT"' EXIT

FAILS=0
pass() { echo "PASS: $1"; }
fail() { echo "FAIL: $1 -- $2"; FAILS=$((FAILS + 1)); }

# run_hook <notch_dir> <stdin>; sets OUT (stdout+stderr combined) and RC.
run_hook() {
  OUT="$(printf '%s' "$2" | NOTCH_DIR="$1" bash "$HOOK" 2>&1)"
  RC=$?
}

# check_quiet <name>: stdout/stderr empty and exit 0.
check_quiet() {
  if [ "$RC" -ne 0 ]; then fail "$1" "exit $RC"; return 1; fi
  if [ -n "$OUT" ]; then fail "$1" "output was: $OUT"; return 1; fi
  return 0
}

new_dir() { mktemp -d "$ROOT/d.XXXXXX"; }

# 1. exact record
D="$(new_dir)/sessions"   # directory does not exist yet: hook must create it
BEFORE=$(date +%s)
run_hook "$D" '{"session_id":"abc123","cwd":"/Users/lem/bianki","hook_event_name":"PermissionRequest","tool_name":"Bash","extra":"ignored"}'
AFTER=$(date +%s)
NAME="writes exact record and creates dir"
if check_quiet "$NAME"; then
  F="$D/abc123.json"
  if [ ! -f "$F" ]; then fail "$NAME" "no $F"
  else
    GOT="$(jq -c '{session_id,cwd,event,notification_type,tool_name}' "$F" 2>&1)"
    WANT='{"session_id":"abc123","cwd":"/Users/lem/bianki","event":"PermissionRequest","notification_type":null,"tool_name":"Bash"}'
    TS_OK="$(jq -r --argjson b "$BEFORE" --argjson a "$AFTER" '(.ts|type=="number") and (.ts==(.ts|floor)) and .ts>=$b and .ts<=$a' "$F" 2>&1)"
    KEYS="$(jq -c 'keys' "$F" 2>&1)"
    if [ "$GOT" != "$WANT" ]; then fail "$NAME" "got $GOT"
    elif [ "$TS_OK" != "true" ]; then fail "$NAME" "ts not integer in [$BEFORE,$AFTER]: $(cat "$F")"
    elif [ "$KEYS" != '["cwd","event","notification_type","session_id","tool_name","ts"]' ]; then fail "$NAME" "keys $KEYS"
    else pass "$NAME"; fi
  fi
fi

# 2. no leftover temp files after a write
NAME="atomic write leaves only the json file"
LS="$(ls -A "$D")"
if [ "$LS" = "abc123.json" ]; then pass "$NAME"; else fail "$NAME" "dir has: $LS"; fi

# 3. later event overwrites earlier
run_hook "$D" '{"session_id":"abc123","cwd":"/Users/lem/bianki","hook_event_name":"Stop"}'
NAME="second event overwrites record"
if check_quiet "$NAME"; then
  GOT="$(jq -c '[.event,.tool_name]' "$D/abc123.json" 2>&1)"
  if [ "$GOT" = '["Stop",null]' ]; then pass "$NAME"; else fail "$NAME" "got $GOT"; fi
fi

# 4. notification type copied
D="$(new_dir)"
run_hook "$D" '{"session_id":"n1","cwd":"/a/b","hook_event_name":"Notification","notification_type":"idle_prompt"}'
NAME="Notification idle_prompt written with type"
if check_quiet "$NAME"; then
  GOT="$(jq -c '[.event,.notification_type]' "$D/n1.json" 2>&1)"
  if [ "$GOT" = '["Notification","idle_prompt"]' ]; then pass "$NAME"; else fail "$NAME" "got $GOT"; fi
fi

# 5. each listed notification type is written
for T in permission_prompt idle_prompt elicitation_dialog agent_needs_input; do
  D="$(new_dir)"
  run_hook "$D" "{\"session_id\":\"t\",\"cwd\":\"/a\",\"hook_event_name\":\"Notification\",\"notification_type\":\"$T\"}"
  NAME="Notification $T is written"
  if check_quiet "$NAME"; then
    if [ -f "$D/t.json" ]; then pass "$NAME"; else fail "$NAME" "no file"; fi
  fi
done

# 6. unlisted notification type skipped
D="$(new_dir)"
run_hook "$D" '{"session_id":"u1","cwd":"/a","hook_event_name":"Notification","notification_type":"auth_success"}'
NAME="unlisted Notification type writes nothing"
if check_quiet "$NAME"; then
  if [ -z "$(ls -A "$D")" ]; then pass "$NAME"; else fail "$NAME" "dir has: $(ls -A "$D")"; fi
fi

# 7. SessionEnd deletes
D="$(new_dir)"
run_hook "$D" '{"session_id":"e1","cwd":"/a","hook_event_name":"SessionStart"}'
NAME="SessionEnd deletes the record"
if [ ! -f "$D/e1.json" ]; then fail "$NAME" "setup write failed"
else
  run_hook "$D" '{"session_id":"e1","cwd":"/a","hook_event_name":"SessionEnd"}'
  if check_quiet "$NAME"; then
    if [ -z "$(ls -A "$D")" ]; then pass "$NAME"; else fail "$NAME" "dir has: $(ls -A "$D")"; fi
  fi
fi

# 8. SessionEnd with nothing to delete
D="$(new_dir)"
run_hook "$D" '{"session_id":"none","cwd":"/a","hook_event_name":"SessionEnd"}'
NAME="SessionEnd with no record is quiet and writes nothing"
if check_quiet "$NAME"; then
  if [ -z "$(ls -A "$D")" ]; then pass "$NAME"; else fail "$NAME" "dir has: $(ls -A "$D")"; fi
fi

# 9. subagent PreToolUse skipped and does not clobber the parent record
D="$(new_dir)"
run_hook "$D" '{"session_id":"sa","cwd":"/a","hook_event_name":"PermissionRequest","tool_name":"Bash"}'
run_hook "$D" '{"session_id":"sa","cwd":"/a","hook_event_name":"PreToolUse","tool_name":"Read","agent_id":"sub1"}'
NAME="subagent PreToolUse is skipped"
if check_quiet "$NAME"; then
  GOT="$(jq -r '.event' "$D/sa.json" 2>&1)"
  if [ "$GOT" = "PermissionRequest" ]; then pass "$NAME"; else fail "$NAME" "event is now $GOT"; fi
fi

# 10. subagent PreToolUse alone writes nothing
D="$(new_dir)"
run_hook "$D" '{"session_id":"sb","cwd":"/a","hook_event_name":"PreToolUse","agent_id":"sub1"}'
NAME="subagent PreToolUse alone writes nothing"
if check_quiet "$NAME"; then
  if [ -z "$(ls -A "$D")" ]; then pass "$NAME"; else fail "$NAME" "dir has: $(ls -A "$D")"; fi
fi

# 11. subagent PermissionRequest written
D="$(new_dir)"
run_hook "$D" '{"session_id":"sc","cwd":"/a","hook_event_name":"PermissionRequest","tool_name":"Edit","agent_id":"sub1"}'
NAME="subagent PermissionRequest is written"
if check_quiet "$NAME"; then
  GOT="$(jq -c '[.event,.tool_name]' "$D/sc.json" 2>&1)"
  if [ "$GOT" = '["PermissionRequest","Edit"]' ]; then pass "$NAME"; else fail "$NAME" "got $GOT"; fi
fi

# 12. subagent Notification (listed type) written
D="$(new_dir)"
run_hook "$D" '{"session_id":"sd","cwd":"/a","hook_event_name":"Notification","notification_type":"permission_prompt","agent_id":"sub1"}'
NAME="subagent listed Notification is written"
if check_quiet "$NAME"; then
  if [ -f "$D/sd.json" ]; then pass "$NAME"; else fail "$NAME" "no file"; fi
fi

# 13. invalid session ids write nothing
i=0
for BAD in '../x' 'a/b' 'a b' 'a.b' '' '..'; do
  i=$((i + 1))
  D="$(new_dir)"
  PAYLOAD="$(jq -cn --arg id "$BAD" '{session_id:$id,cwd:"/a",hook_event_name:"Stop"}')"
  run_hook "$D" "$PAYLOAD"
  NAME="invalid session_id '$BAD' writes nothing"
  if check_quiet "$NAME"; then
    # also check the parent dir for an escaped file
    if [ -z "$(ls -A "$D")" ] && [ ! -e "$D/../x.json" ]; then pass "$NAME"; else fail "$NAME" "dir has: $(ls -A "$D")"; fi
  fi
done

# 14. missing session_id
D="$(new_dir)"
run_hook "$D" '{"cwd":"/a","hook_event_name":"Stop"}'
NAME="missing session_id writes nothing"
if check_quiet "$NAME"; then
  if [ -z "$(ls -A "$D")" ]; then pass "$NAME"; else fail "$NAME" "dir has: $(ls -A "$D")"; fi
fi

# 15. SessionEnd with traversal id deletes nothing outside the dir
D="$(new_dir)"
VICTIM="$D/victim.json"
mkdir -p "$D/sub"
echo '{}' > "$VICTIM"
run_hook "$D/sub" "$(jq -cn --arg id "../victim" '{session_id:$id,cwd:"/a",hook_event_name:"SessionEnd"}')"
NAME="SessionEnd with traversal id deletes nothing"
if check_quiet "$NAME"; then
  if [ -f "$VICTIM" ]; then pass "$NAME"; else fail "$NAME" "victim file deleted"; fi
fi

# 16. malformed JSON
D="$(new_dir)"
run_hook "$D" '{not json at all'
NAME="malformed JSON is quiet, exit 0, no write"
if check_quiet "$NAME"; then
  if [ -z "$(ls -A "$D")" ]; then pass "$NAME"; else fail "$NAME" "dir has: $(ls -A "$D")"; fi
fi

# 17. empty stdin
D="$(new_dir)"
run_hook "$D" ''
NAME="empty stdin is quiet, exit 0"
if check_quiet "$NAME"; then pass "$NAME"; fi

# 18. unwritable directory
D="$(new_dir)"
chmod 500 "$D"
run_hook "$D/sessions" '{"session_id":"w1","cwd":"/a","hook_event_name":"Stop"}'
NAME="unwritable NOTCH_DIR is quiet, exit 0"
if [ "$(id -u)" = "0" ]; then
  echo "SKIP: $NAME (running as root)"
elif check_quiet "$NAME"; then
  if [ -z "$(ls -A "$D")" ]; then pass "$NAME"; else fail "$NAME" "dir has: $(ls -A "$D")"; fi
fi
chmod 700 "$D"

# 19. every contract event is written with stdout empty
for EV in SessionStart UserPromptSubmit PreToolUse PostToolUse PostToolUseFailure PermissionRequest Stop StopFailure; do
  D="$(new_dir)"
  run_hook "$D" "{\"session_id\":\"ev\",\"cwd\":\"/a\",\"hook_event_name\":\"$EV\"}"
  NAME="$EV is written, stdout empty"
  if check_quiet "$NAME"; then
    GOT="$(jq -r '.event' "$D/ev.json" 2>&1)"
    if [ "$GOT" = "$EV" ]; then pass "$NAME"; else fail "$NAME" "event $GOT"; fi
  fi
done

if [ "$FAILS" -eq 0 ]; then echo "ALL PASS"; exit 0; fi
echo "$FAILS FAILED"
exit 1
