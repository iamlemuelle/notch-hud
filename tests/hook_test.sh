#!/bin/bash
# Behavior tests for hooks/notch-hook.sh, written against CONTRACT.md section 1.
HERE="$(cd "$(dirname "$0")" && pwd)"
HOOK="$HERE/../hooks/notch-hook.sh"
ROOT="$(mktemp -d)"
trap 'chmod -R u+rwx "$ROOT" 2>/dev/null; rm -rf "$ROOT"' EXIT

# The suite may itself run inside a Claude Code session: start every case from an environment without the two
# variables the hook copies into inbox and host, and set them per case.
unset CLAUDE_CODE_MESSAGING_SOCKET CLAUDE_CODE_MESSAGING_TOKEN __CFBundleIdentifier

FAILS=0
pass() { echo "PASS: $1"; }
fail() { echo "FAIL: $1 -- $2"; FAILS=$((FAILS + 1)); }

# run_hook <notch_dir> <stdin>; sets OUT (stdout+stderr combined) and RC.
# Executes the script directly, as Claude Code does (its #!/bin/sh), not through bash.
run_hook() {
  OUT="$(printf '%s' "$2" | NOTCH_DIR="$1" "$HOOK" 2>&1)"
  RC=$?
}

# check_quiet <name>: stdout/stderr empty and exit 0.
check_quiet() {
  if [ "$RC" -ne 0 ]; then fail "$1" "exit $RC"; return 1; fi
  if [ -n "$OUT" ]; then fail "$1" "output was: $OUT"; return 1; fi
  return 0
}

new_dir() { mktemp -d "$ROOT/d.XXXXXX"; }

# ls_line <dir>: sorted file names joined by spaces.
ls_line() { ls -A "$1" | sort | tr '\n' ' ' | sed 's/ $//'; }

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
    GOT="$(jq -c '{session_id,agent_id,cwd,event,notification_type,tool_name,inbox,host}' "$F" 2>&1)"
    WANT='{"session_id":"abc123","agent_id":null,"cwd":"/Users/lem/bianki","event":"PermissionRequest","notification_type":null,"tool_name":"Bash","inbox":null,"host":null}'
    TS_OK="$(jq -r --argjson b "$BEFORE" --argjson a "$AFTER" '(.ts|type=="number") and (.ts==(.ts|floor)) and .ts>=$b and .ts<=$a' "$F" 2>&1)"
    KEYS="$(jq -c 'keys' "$F" 2>&1)"
    if [ "$GOT" != "$WANT" ]; then fail "$NAME" "got $GOT"
    elif [ "$TS_OK" != "true" ]; then fail "$NAME" "ts not integer in [$BEFORE,$AFTER]: $(cat "$F")"
    elif [ "$KEYS" != '["agent_id","cwd","event","host","inbox","notification_type","session_id","tool_name","ts"]' ]; then fail "$NAME" "keys $KEYS"
    else pass "$NAME"; fi
  fi
fi

# 2. no leftover temp files after a write
NAME="atomic write leaves only the json file"
LS="$(ls_line "$D")"
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

# 9. subagent PreToolUse goes to its own file; the main record is untouched
D="$(new_dir)"
run_hook "$D" '{"session_id":"sa","cwd":"/a","hook_event_name":"PermissionRequest","tool_name":"Bash"}'
run_hook "$D" '{"session_id":"sa","cwd":"/a","hook_event_name":"PreToolUse","tool_name":"Read","agent_id":"sub1"}'
NAME="subagent PreToolUse is written to <sid>.<agent>.json, main file untouched"
if check_quiet "$NAME"; then
  MAIN="$(jq -c '[.event,.tool_name,.agent_id]' "$D/sa.json" 2>&1)"
  SUBR="$(jq -c '[.event,.tool_name,.agent_id,.session_id]' "$D/sa.sub1.json" 2>&1)"
  LS="$(ls_line "$D")"
  if [ "$MAIN" != '["PermissionRequest","Bash",null]' ]; then fail "$NAME" "main is $MAIN"
  elif [ "$SUBR" != '["PreToolUse","Read","sub1","sa"]' ]; then fail "$NAME" "sub is $SUBR"
  elif [ "$LS" != "sa.json sa.sub1.json" ]; then fail "$NAME" "dir has: $LS"
  else pass "$NAME"; fi
fi

# 10. subagent PreToolUse alone writes only the agent file
D="$(new_dir)"
run_hook "$D" '{"session_id":"sb","cwd":"/a","hook_event_name":"PreToolUse","agent_id":"sub1"}'
NAME="subagent PreToolUse alone writes only <sid>.<agent>.json"
if check_quiet "$NAME"; then
  LS="$(ls_line "$D")"
  if [ "$LS" = "sb.sub1.json" ]; then pass "$NAME"; else fail "$NAME" "dir has: $LS"; fi
fi

# 11. subagent PermissionRequest written
D="$(new_dir)"
run_hook "$D" '{"session_id":"sc","cwd":"/a","hook_event_name":"PermissionRequest","tool_name":"Edit","agent_id":"sub1"}'
NAME="subagent PermissionRequest is written to its agent file"
if check_quiet "$NAME"; then
  GOT="$(jq -c '[.event,.tool_name,.agent_id]' "$D/sc.sub1.json" 2>&1)"
  if [ "$GOT" = '["PermissionRequest","Edit","sub1"]' ] && [ ! -e "$D/sc.json" ]; then pass "$NAME"; else fail "$NAME" "got $GOT, dir: $(ls_line "$D")"; fi
fi

# 12. subagent Notification (listed type) written
D="$(new_dir)"
run_hook "$D" '{"session_id":"sd","cwd":"/a","hook_event_name":"Notification","notification_type":"permission_prompt","agent_id":"sub1"}'
NAME="subagent listed Notification is written to its agent file"
if check_quiet "$NAME"; then
  if [ -f "$D/sd.sub1.json" ] && [ ! -e "$D/sd.json" ]; then pass "$NAME"; else fail "$NAME" "dir: $(ls_line "$D")"; fi
fi

# 13. subagent unlisted Notification skipped
D="$(new_dir)"
run_hook "$D" '{"session_id":"se","cwd":"/a","hook_event_name":"Notification","notification_type":"auth_success","agent_id":"sub1"}'
NAME="subagent unlisted Notification writes nothing"
if check_quiet "$NAME"; then
  if [ -z "$(ls -A "$D")" ]; then pass "$NAME"; else fail "$NAME" "dir has: $(ls -A "$D")"; fi
fi

# 14. explicit null agent_id is the main thread
D="$(new_dir)"
run_hook "$D" '{"session_id":"sn","cwd":"/a","hook_event_name":"Stop","agent_id":null}'
NAME="agent_id null writes <sid>.json"
if check_quiet "$NAME"; then
  if [ "$(ls_line "$D")" = "sn.json" ]; then pass "$NAME"; else fail "$NAME" "dir has: $(ls_line "$D")"; fi
fi

# 15. two agents of one session coexist; a later event overwrites only its own file
D="$(new_dir)"
run_hook "$D" '{"session_id":"sf","cwd":"/a","hook_event_name":"PreToolUse","agent_id":"a1"}'
run_hook "$D" '{"session_id":"sf","cwd":"/a","hook_event_name":"PreToolUse","agent_id":"a2"}'
run_hook "$D" '{"session_id":"sf","cwd":"/a","hook_event_name":"PermissionRequest","tool_name":"Bash","agent_id":"a1"}'
NAME="agent files coexist and overwrite independently"
E1="$(jq -r '.event' "$D/sf.a1.json" 2>&1)"
E2="$(jq -r '.event' "$D/sf.a2.json" 2>&1)"
if [ "$E1/$E2" = "PermissionRequest/PreToolUse" ] && [ "$(ls_line "$D")" = "sf.a1.json sf.a2.json" ]; then pass "$NAME"; else fail "$NAME" "got $E1/$E2, dir: $(ls_line "$D")"; fi

# 16. SubagentStop deletes only that agent's file
D="$(new_dir)"
run_hook "$D" '{"session_id":"sg","cwd":"/a","hook_event_name":"Stop"}'
run_hook "$D" '{"session_id":"sg","cwd":"/a","hook_event_name":"PreToolUse","agent_id":"a1"}'
run_hook "$D" '{"session_id":"sg","cwd":"/a","hook_event_name":"PreToolUse","agent_id":"a2"}'
run_hook "$D" '{"session_id":"sh","cwd":"/a","hook_event_name":"PreToolUse","agent_id":"a1"}'
run_hook "$D" '{"session_id":"sg","cwd":"/a","hook_event_name":"SubagentStop","agent_id":"a1"}'
NAME="SubagentStop deletes only that agent's file"
if check_quiet "$NAME"; then
  LS="$(ls_line "$D")"
  if [ "$LS" = "sg.a2.json sg.json sh.a1.json" ]; then pass "$NAME"; else fail "$NAME" "dir has: $LS"; fi
fi

# 17. SessionEnd deletes the main and every agent file of that session, nothing else
D="$(new_dir)"
for ID in ab abc; do
  run_hook "$D" "{\"session_id\":\"$ID\",\"cwd\":\"/a\",\"hook_event_name\":\"Stop\"}"
  run_hook "$D" "{\"session_id\":\"$ID\",\"cwd\":\"/a\",\"hook_event_name\":\"PreToolUse\",\"agent_id\":\"a1\"}"
done
run_hook "$D" '{"session_id":"ab","cwd":"/a","hook_event_name":"PreToolUse","agent_id":"a2"}'
run_hook "$D" '{"session_id":"ab","cwd":"/a","hook_event_name":"SessionEnd"}'
NAME="SessionEnd deletes main and all agent files, only for that session"
if check_quiet "$NAME"; then
  LS="$(ls_line "$D")"
  if [ "$LS" = "abc.a1.json abc.json" ]; then pass "$NAME"; else fail "$NAME" "dir has: $LS"; fi
fi

# 18. SubagentStop with no file is quiet
D="$(new_dir)"
run_hook "$D" '{"session_id":"sx","cwd":"/a","hook_event_name":"SubagentStop","agent_id":"a1"}'
NAME="SubagentStop with no file is quiet and writes nothing"
if check_quiet "$NAME"; then
  if [ -z "$(ls -A "$D")" ]; then pass "$NAME"; else fail "$NAME" "dir has: $(ls -A "$D")"; fi
fi

# 19. invalid agent ids write nothing and delete nothing
for BAD in '../x' 'a/b' 'a b' 'a.b' $'a1\n' '..'; do
  D="$(new_dir)"
  run_hook "$D" '{"session_id":"sy","cwd":"/a","hook_event_name":"Stop"}'
  PAYLOAD="$(jq -cn --arg id "$BAD" '{session_id:"sy",cwd:"/a",hook_event_name:"PreToolUse",agent_id:$id}')"
  run_hook "$D" "$PAYLOAD"
  NAME="invalid agent_id $(printf '%q' "$BAD") writes nothing"
  if check_quiet "$NAME"; then
    if [ "$(ls_line "$D")" = "sy.json" ] && [ ! -e "$D/../x.json" ]; then pass "$NAME"; else fail "$NAME" "dir has: $(ls_line "$D")"; fi
  fi
  PAYLOAD="$(jq -cn --arg id "$BAD" '{session_id:"sy",cwd:"/a",hook_event_name:"SubagentStop",agent_id:$id}')"
  run_hook "$D" "$PAYLOAD"
  NAME="SubagentStop with invalid agent_id $(printf '%q' "$BAD") deletes nothing"
  if check_quiet "$NAME"; then
    if [ "$(ls_line "$D")" = "sy.json" ]; then pass "$NAME"; else fail "$NAME" "dir has: $(ls_line "$D")"; fi
  fi
done

# 20. invalid session ids write nothing (a trailing newline is invalid)
for BAD in '../x' 'a/b' 'a b' 'a.b' '' '..' $'abc\n'; do
  D="$(new_dir)"
  PAYLOAD="$(jq -cn --arg id "$BAD" '{session_id:$id,cwd:"/a",hook_event_name:"Stop"}')"
  run_hook "$D" "$PAYLOAD"
  NAME="invalid session_id $(printf '%q' "$BAD") writes nothing"
  if check_quiet "$NAME"; then
    # also check the parent dir for an escaped file
    if [ -z "$(ls -A "$D")" ] && [ ! -e "$D/../x.json" ]; then pass "$NAME"; else fail "$NAME" "dir has: $(ls_line "$D")"; fi
  fi
done

# 21. missing session_id
D="$(new_dir)"
run_hook "$D" '{"cwd":"/a","hook_event_name":"Stop"}'
NAME="missing session_id writes nothing"
if check_quiet "$NAME"; then
  if [ -z "$(ls -A "$D")" ]; then pass "$NAME"; else fail "$NAME" "dir has: $(ls -A "$D")"; fi
fi

# 22. SessionEnd with traversal id deletes nothing outside the dir
D="$(new_dir)"
VICTIM="$D/victim.json"
mkdir -p "$D/sub"
echo '{}' > "$VICTIM"
run_hook "$D/sub" "$(jq -cn --arg id "../victim" '{session_id:$id,cwd:"/a",hook_event_name:"SessionEnd"}')"
NAME="SessionEnd with traversal id deletes nothing"
if check_quiet "$NAME"; then
  if [ -f "$VICTIM" ]; then pass "$NAME"; else fail "$NAME" "victim file deleted"; fi
fi

# 23. malformed JSON
D="$(new_dir)"
run_hook "$D" '{not json at all'
NAME="malformed JSON is quiet, exit 0, no write"
if check_quiet "$NAME"; then
  if [ -z "$(ls -A "$D")" ]; then pass "$NAME"; else fail "$NAME" "dir has: $(ls -A "$D")"; fi
fi

# 24. empty stdin
D="$(new_dir)"
run_hook "$D" ''
NAME="empty stdin is quiet, exit 0"
if check_quiet "$NAME"; then pass "$NAME"; fi

# 25. unwritable directory
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

# 26. every contract event is written with stdout empty
for EV in SessionStart UserPromptSubmit PreToolUse PostToolUse PostToolUseFailure PermissionRequest PermissionDenied Stop StopFailure; do
  D="$(new_dir)"
  run_hook "$D" "{\"session_id\":\"ev\",\"cwd\":\"/a\",\"hook_event_name\":\"$EV\"}"
  NAME="$EV is written, stdout empty"
  if check_quiet "$NAME"; then
    GOT="$(jq -r '.event' "$D/ev.json" 2>&1)"
    if [ "$GOT" = "$EV" ]; then pass "$NAME"; else fail "$NAME" "event $GOT"; fi
  fi
done

# 27. missing jq: quiet, exit 0, no write (PATH holds every tool except jq)
NOJQ="$ROOT/nojq-bin"
mkdir -p "$NOJQ"
for DIR in /bin /usr/bin /usr/sbin /sbin; do
  for T in "$DIR"/*; do
    B="${T##*/}"
    [ "$B" = "jq" ] || [ -e "$NOJQ/$B" ] || ln -s "$T" "$NOJQ/$B" 2>/dev/null
  done
done
D="$(new_dir)"
OUT="$(printf '%s' '{"session_id":"nj","cwd":"/a","hook_event_name":"Stop"}' | PATH="$NOJQ" NOTCH_DIR="$D" "$HOOK" 2>&1)"
RC=$?
NAME="missing jq is quiet, exit 0"
if PATH="$NOJQ" command -v jq >/dev/null 2>&1; then echo "SKIP: $NAME (jq still on PATH)"
elif check_quiet "$NAME"; then pass "$NAME"; fi

# 28. inbox and host are copied from the environment: set gives the string, unset or empty gives null
# field_case <name> <want inbox,host as JSON> [VAR=value ...]
field_case() {
  NAME="$1"; WANT="$2"; shift 2
  D="$(new_dir)"
  OUT="$(printf '%s' '{"session_id":"ib","cwd":"/a","hook_event_name":"Stop"}' | env "$@" NOTCH_DIR="$D" "$HOOK" 2>&1)"
  RC=$?
  if check_quiet "$NAME"; then
    GOT="$(jq -c '[.inbox,.host]' "$D/ib.json" 2>&1)"
    if [ "$GOT" = "$WANT" ]; then pass "$NAME"; else fail "$NAME" "got $GOT"; fi
  fi
}
field_case "inbox from CLAUDE_CODE_MESSAGING_SOCKET" '["/tmp/x.sock",null]' CLAUDE_CODE_MESSAGING_SOCKET=/tmp/x.sock
field_case "empty CLAUDE_CODE_MESSAGING_SOCKET gives null inbox" '[null,null]' CLAUDE_CODE_MESSAGING_SOCKET=
field_case "host from __CFBundleIdentifier" '[null,"com.anthropic.claudefordesktop"]' __CFBundleIdentifier=com.anthropic.claudefordesktop
field_case "empty __CFBundleIdentifier gives null host" '[null,null]' __CFBundleIdentifier=
field_case "both set" '["/tmp/cc-socks/1.sock","com.apple.Terminal"]' CLAUDE_CODE_MESSAGING_SOCKET=/tmp/cc-socks/1.sock __CFBundleIdentifier=com.apple.Terminal
field_case "both unset gives nulls" '[null,null]'

# 29. a value with a quote, a backslash, a newline and shell syntax stays one JSON string and runs nothing
PWNED="$ROOT/pwned"
for VAR in CLAUDE_CODE_MESSAGING_SOCKET __CFBundleIdentifier; do
  V="/tmp/a\"b\\c
d\$(touch $PWNED)\`touch $PWNED\`.sock"
  D="$(new_dir)"
  OUT="$(printf '%s' '{"session_id":"q","cwd":"/a","hook_event_name":"Stop"}' | env "$VAR=$V" NOTCH_DIR="$D" "$HOOK" 2>&1)"
  RC=$?
  FIELD=inbox
  [ "$VAR" = "CLAUDE_CODE_MESSAGING_SOCKET" ] || FIELD=host
  NAME="$VAR with quote, backslash, newline is one JSON string in $FIELD"
  if check_quiet "$NAME"; then
    SAME="$(jq --arg v "$V" --arg f "$FIELD" '.[$f] == $v' "$D/q.json" 2>&1)"
    LINES="$(wc -l <"$D/q.json" | tr -d ' ')"
    if [ "$SAME" != "true" ]; then fail "$NAME" "file: $(cat "$D/q.json")"
    elif [ "$LINES" != "1" ]; then fail "$NAME" "$LINES lines"
    elif [ -e "$PWNED" ]; then fail "$NAME" "the value ran as a command"
    else pass "$NAME"; fi
  fi
done

# 30. timing: median of 100 runs with both fields set stays within the 20 ms budget (docs/PLAN.md section 7).
# The budget is this Mac's; a CI runner starts processes slower (25.1 ms there with ~15 ms here), so CI only
# reports the median. Every run must still exit 0 and yield a number.
D="$(new_dir)"
IN="$ROOT/timing.json"
printf '%s' '{"session_id":"tm","cwd":"/Users/lem/bianki","hook_event_name":"PreToolUse","tool_name":"Bash"}' >"$IN"
MEDIAN="$(CLAUDE_CODE_MESSAGING_SOCKET=/tmp/cc-socks/1.sock __CFBundleIdentifier=com.apple.Terminal NOTCH_DIR="$D" \
  /usr/bin/perl -MTime::HiRes=time -e '
    my ($hook, $in) = @ARGV;
    my @ms;
    for (1 .. 100) {
      open(STDIN, "<", $in) or die;
      my $t = time;
      system($hook) == 0 or die "exit $?";
      push @ms, (time - $t) * 1000;
    }
    @ms = sort { $a <=> $b } @ms;
    printf "%.1f", ($ms[49] + $ms[50]) / 2;
  ' "$HOOK" "$IN" 2>&1)"
NAME="median of 100 runs is at most 20 ms"
if ! awk -v m="$MEDIAN" 'BEGIN { exit !(m ~ /^[0-9]+\.[0-9]$/) }'; then fail "$NAME" "no median: [$MEDIAN]"
elif [ -n "${CI:-}" ]; then echo "INFO: hook median of 100 runs is $MEDIAN ms (CI: reported, not held to 20 ms)"
elif awk -v m="$MEDIAN" 'BEGIN { exit !(m <= 20) }'; then pass "$NAME ($MEDIAN ms)"
else fail "$NAME" "median [$MEDIAN] ms"; fi

if [ "$FAILS" -eq 0 ]; then echo "ALL PASS"; exit 0; fi
echo "$FAILS FAILED"
exit 1
