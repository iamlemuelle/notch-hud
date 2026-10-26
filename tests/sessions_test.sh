#!/bin/bash
# End to end, on the real release app (CONTRACT.md section 3). Checks, in order:
#   - startup: the seed session shows in a `sessions:` line;
#   - five Stop rounds: each Stop reaches a `sessions:` line as done within 300 ms (file-system events, not a poll);
#   - SessionEnd removes the seed row and then the session row, each within 300 ms;
#   - idle for 3 s: no new `sessions:` line;
#   - deadline decay: a record 2 s short of the 600 s quiet limit turns working to waiting within 4.5 s with no file
#     event, after a `refresh: deadline` line, and at most 2 such lines between the working and waiting rows;
#   - idle for 11 s with one kept Stop record (next_wake is Some, about 3 h away): every refresh is a
#     `refresh: rescan`, 2 or 3 of them (no poll, no deadline loop);
#   - deleted and recreated directory: the first hook write, and a second write (another session, Stop) once the
#     directory exists again, each reach the trace within 300 ms.
#
# It does not move the cursor and needs no Screen Recording. It starts its own app and only ever signals that pid; the
# notch-hud that may be running as your login item is left alone.
#
# Exit 0 when every check passes, 1 when one fails, 2 when the environment cannot run it (no notched display, build
# failure, no jq).
set -u
unset CLAUDE_CODE_MESSAGING_SOCKET CLAUDE_CODE_MESSAGING_TOKEN  # never let a test reach a real session's inbox
root="$(cd "$(dirname "$0")/.." && pwd)"
export PATH="/opt/homebrew/opt/rustup/bin:$PATH"
fail=0
pass() { echo "PASS $1"; }
flunk() { echo "FAIL $1"; fail=1; }

tmp="$(mktemp -d)"
app=""
cleanup() {
  [ -n "$app" ] && kill "$app" 2>/dev/null
  wait 2>/dev/null
}
trap cleanup EXIT

(cd "$root/src-tauri" && cargo build --release -q) || { echo "SKIP app build failed"; exit 2; }

export NOTCH_DIR="$tmp/sessions"
export NOTCH_TRACE=1
applog="$tmp/app.log"
: >"$applog"

now() { perl -MTime::HiRes=time -e 'printf "%.3f", time'; }
lines() { wc -l <"$1" | tr -d ' '; }
hook() { printf '%s' "$1" | "$root/hooks/notch-hook.sh"; }
ev() { # ev <session> <event> [extra json fields]
  hook "{\"session_id\":\"$1\",\"cwd\":\"/x/$1\",\"hook_event_name\":\"$2\"${3:-}}"
}
# wait_line <fixed string> <secs> <after line>: waits for a `sessions:` line containing the string; sets $hit_ts.
hit_ts=""
wait_line() {
  local want="$1" secs="$2" after="$3" i=0 line
  while [ "$i" -lt $((secs * 50)) ]; do
    line="$(tail -n +$((after + 1)) "$applog" | grep -F 'sessions: ' | grep -F -- "$want" | head -n 1)"
    if [ -n "$line" ]; then hit_ts="${line%% *}"; return 0; fi
    sleep 0.02; i=$((i + 1))
  done
  return 1
}
# measure <label> <want> <after line> <t0>: waits up to 2 s for the line, then checks the latency against 300 ms.
measure() {
  local label="$1" want="$2" after="$3" t0="$4" ms
  if ! wait_line "$want" 2 "$after"; then flunk "$label: no sessions: line containing $want within 2 s"; return 1; fi
  ms="$(awk -v a="$t0" -v b="$hit_ts" 'BEGIN { printf "%.0f", (b - a) * 1000 }')"
  if [ "$ms" -lt 300 ]; then pass "$label: ${ms} ms (under 300)"; else flunk "$label: ${ms} ms (want under 300)"; fi
}

command -v /usr/bin/jq >/dev/null || { echo "SKIP jq is not installed"; exit 2; }

"$root/src-tauri/target/release/notch-hud" 2> >(perl -MTime::HiRes=time -ne 'BEGIN { $| = 1 } printf "%.3f %s", time, $_' >"$applog") & app=$!

# Wait for the page, or for the app to say there is no notched display.
i=0; loaded=0
while [ "$i" -lt 300 ]; do
  if grep -qF 'no notched display' "$applog"; then echo "SKIP no notched display"; exit 2; fi
  if grep -qF 'page: loaded' "$applog"; then loaded=1; break; fi
  kill -0 "$app" 2>/dev/null || { sleep 0.3; grep -qF 'no notched display' "$applog" && { echo "SKIP no notched display"; exit 2; }; break; }
  sleep 0.1; i=$((i + 1))
done
if [ "$loaded" != 1 ]; then flunk "page loads"; echo "--- app.log"; cat "$applog"; exit 1; fi
pass "page loaded"

# Startup: the seed session written before the first event must show in a sessions: line (bounded wait).
ev seed SessionStart
if wait_line '"label":"seed"' 10 0; then pass "a sessions: line shows the seed session"; else flunk "no sessions: line with the seed session within 10 s"; echo "--- app.log"; cat "$applog"; exit 1; fi
sleep 0.5

# Working, then Stop: five rounds, each Stop must show as done within 300 ms.
for n in 1 2 3 4 5; do
  m="$(lines "$applog")"
  if [ $((n % 2)) = 1 ]; then ev sess PreToolUse ',"tool_name":"Read","tool_input":{}'; else ev sess UserPromptSubmit ',"prompt":"go"'; fi
  wait_line '"label":"sess","status":"working"' 2 "$m" || flunk "round $n: the working row did not appear within 2 s"
  sleep 0.4
  m="$(lines "$applog")"
  t0="$(now)"
  ev sess Stop
  measure "round $n: Stop shows done" '"label":"sess","status":"done"' "$m" "$t0"
  sleep 0.4
done

# SessionEnd removes the rows (the seed first, then the session under test).
m="$(lines "$applog")"; t0="$(now)"
ev seed SessionEnd
measure "seed SessionEnd keeps one row" '"total":1' "$m" "$t0"
sleep 0.4
m="$(lines "$applog")"; t0="$(now)"
ev sess SessionEnd
measure "SessionEnd removes the row" '"total":0' "$m" "$t0"

# Idle: no writes for 3 s, and no new sessions: line.
sleep 0.5
m="$(lines "$applog")"
sleep 3
extra="$(tail -n +$((m + 1)) "$applog" | grep -cF 'sessions: ')"
if [ "$extra" = 0 ]; then pass "idle for 3 s: no sessions: line"; else flunk "idle for 3 s: $extra new sessions: line(s)"; fi

# Deadline decay with no file event: a record written straight into the directory (the hook would stamp ts = now),
# 2 s short of the 600 s quiet limit. Nothing else is written, so only the deadline can turn it to waiting.
mkdir -p "$NOTCH_DIR"
m="$(lines "$applog")"
ts_old=$(( $(date +%s) - 598 ))
printf '{"session_id":"decay","agent_id":null,"cwd":"/x/decay","event":"PreToolUse","notification_type":null,"tool_name":null,"ts":%s}\n' "$ts_old" >"$NOTCH_DIR/.decay.tmp"
mv -f "$NOTCH_DIR/.decay.tmp" "$NOTCH_DIR/decay.json"
if wait_line '"label":"decay","status":"working"' 2 "$m"; then
  work_ts="$hit_ts"
  if wait_line '"label":"decay","status":"waiting"' 5 "$m"; then
    wait_ts="$hit_ts"
    delay="$(awk -v a="$work_ts" -v b="$wait_ts" 'BEGIN { printf "%.2f", b - a }')"
    dl="$(tail -n +$((m + 1)) "$applog" | grep -nF 'refresh: deadline' | head -n 1 | cut -d: -f1)"
    wl="$(tail -n +$((m + 1)) "$applog" | grep -nF 'sessions: ' | grep -F '"label":"decay","status":"waiting"' | head -n 1 | cut -d: -f1)"
    wk="$(tail -n +$((m + 1)) "$applog" | grep -nF 'sessions: ' | grep -F '"label":"decay","status":"working"' | head -n 1 | cut -d: -f1)"
    ndl="$(tail -n +$((m + 1)) "$applog" | sed -n "${wk},${wl}p" | grep -cF 'refresh: deadline')"
    if [ -z "$dl" ]; then flunk "deadline: no 'refresh: deadline' line before the waiting row"
    elif [ "$ndl" -gt 2 ]; then flunk "deadline: $ndl 'refresh: deadline' lines between working and waiting (want at most 2)"
    elif [ "$dl" -ge "$wl" ]; then flunk "deadline: 'refresh: deadline' (line $dl) is not before the waiting row (line $wl)"
    elif awk -v d="$delay" 'BEGIN { exit !(d <= 4.5) }'; then pass "deadline: working to waiting with no file event in ${delay} s (within 4.5), after refresh: deadline (${ndl} deadline line(s) in between, at most 2)"
    else flunk "deadline: working to waiting took ${delay} s (want within 4.5)"; fi
  else flunk "deadline: the decay row never became waiting within 5 s of the working line"; fi
else flunk "deadline: the decay row did not show as working within 2 s"; fi

# Idle: drop the decay session (no Working session is left), keep one Stop record so next_wake is Some (a TTL about
# 3 h away), then 11 s with no writes. Every refresh must be a rescan (RESCAN is 5 s), so 2 or 3 of them: not a 1 s
# poll, not a refresh loop.
ev decay SessionEnd
sleep 0.5
ev keep Stop
sleep 1
m="$(lines "$applog")"
sleep 11
refreshes="$(tail -n +$((m + 1)) "$applog" | grep -F 'refresh: ')"
total_r="$(printf '%s' "$refreshes" | grep -c . )"
rescans="$(printf '%s' "$refreshes" | grep -cF 'refresh: rescan')"
if [ "$total_r" = "$rescans" ] && [ "$rescans" -ge 2 ] && [ "$rescans" -le 3 ]; then pass "idle for 11 s with a kept Stop record: $rescans refresh: rescan line(s), no other refresh"
else flunk "idle for 11 s: $total_r refresh line(s), $rescans rescan (want 2 or 3, all rescan)"; printf '%s\n' "$refreshes"; fi

# Deleted and recreated directory: the first hook write recreates it, and a second write inside the new directory
# must also reach the trace within 300 ms (file events still arrive, not only the mkdir).
ev keep SessionEnd
sleep 0.5
rm -rf "$NOTCH_DIR"
sleep 1
m="$(lines "$applog")"; t0="$(now)"
ev recreated Stop
measure "recreated directory: first Stop shows" '"label":"recreated"' "$m" "$t0"
sleep 1
m="$(lines "$applog")"; t0="$(now)"
ev recreated2 Stop
measure "recreated directory: second write shows" '"label":"recreated2"' "$m" "$t0"

if [ "$fail" -ne 0 ]; then echo "--- app.log"; cat "$applog"; fi
exit "$fail"
