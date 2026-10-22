#!/bin/bash
# End to end, on the real release app (CONTRACT.md section 10, and the section 4 and 8 lines that point to it). Checks:
#   - `notch-hud --check-avatar <path>`: `ok` on stdout and exit 0 for a valid file; exit 1 with the reason on stderr for
#     an invalid file and for a missing one; it does not start the app (no sessions directory is created);
#   - startup log lines on stderr: a missing file and an invalid file each log
#     `avatar: <path>: <reason>, using the default` (the first without NOTCH_TRACE: always), a valid file logs
#     `avatar: loaded <path>`;
#   - moods traced as `avatar: <mood>` follow sessions written through the real hook: working, then a permission
#     request (alert), then Stop (celebrate, and about 4 s later the quiet mood), then a failed stop (error).
#     The quiet mood is `idle`, or `listening` when this app's own `media:` trace shows a track playing: real Now
#     Playing is system wide, so the test reads it instead of assuming silence.
#
# It moves no cursor and needs no Screen Recording. It starts its own apps, with its own NOTCH_DIR, NOTCH_AVATAR and
# adapter, and only ever signals those pids; a notch-hud running as your login item is left alone.
#
# Exit 0 when every check passes, 1 when one fails, 2 when the environment cannot run it (no notched display, build
# failure, no jq) and nothing failed.
set -u
root="$(cd "$(dirname "$0")/.." && pwd)"
export PATH="/opt/homebrew/opt/rustup/bin:$PATH"
fail=0
pass() { echo "PASS $1"; }
flunk() { echo "FAIL $1"; fail=1; }

tmp="$(mktemp -d)"
# Every hook call and app below uses this directory; nothing here may touch ~/.claude/notch/sessions.
export NOTCH_DIR="$tmp/sessions"
app=""
cleanup() {
  [ -n "$app" ] && { kill "$app" 2>/dev/null; wait "$app" 2>/dev/null; }
}
trap cleanup EXIT

command -v /usr/bin/jq >/dev/null || { echo "SKIP jq is not installed"; exit 2; }
(cd "$root/src-tauri" && cargo build --release -q) || { echo "SKIP app build failed"; exit 2; }
bin="$root/src-tauri/target/release/notch-hud"
[ -x "$bin" ] || { echo "SKIP no release binary at $bin"; exit 2; }
fixture="$root/tests/avatar/fixtures/ball.json"
[ -f "$fixture" ] || { echo "FAIL missing fixture $fixture"; exit 1; }

# --- --check-avatar -------------------------------------------------------------------------------------------------
# bounded <cmd...>: a build that ignores the flag would start the app and never exit, so give it 10 s.
bounded() { perl -e 'alarm 10; exec @ARGV' -- "$@"; }
sess="$tmp/check-sessions"
out="$(NOTCH_DIR="$sess" bounded "$bin" --check-avatar "$fixture" 2>"$tmp/ck.err")"; rc=$?
if [ "$rc" = 0 ] && [ "$out" = "ok" ] && [ ! -s "$tmp/ck.err" ]; then pass "--check-avatar on a valid file: ok, exit 0, nothing on stderr"
else flunk "--check-avatar on a valid file: exit $rc, stdout [$out], stderr [$(cat "$tmp/ck.err")]"; fi

jq '.animations.idle.steps[0].expression = "nap"' "$fixture" >"$tmp/bad.json"
out="$(NOTCH_DIR="$sess" bounded "$bin" --check-avatar "$tmp/bad.json" 2>"$tmp/ck.err")"; rc=$?
reason="$(cat "$tmp/ck.err")"
case "$reason" in
  "/animations/idle/steps/0/expression: "*) reasonok=1 ;;
  *) reasonok=0 ;;
esac
if [ "$rc" = 1 ] && [ -z "$out" ] && [ "$reasonok" = 1 ]; then pass "--check-avatar on an invalid file: exit 1, nothing on stdout, stderr starts with the member's pointer ($reason)"
else flunk "--check-avatar on an invalid file: exit $rc, stdout [$out], stderr [$reason]"; fi

out="$(NOTCH_DIR="$sess" bounded "$bin" --check-avatar "$tmp/does-not-exist.json" 2>"$tmp/ck.err")"; rc=$?
if [ "$rc" = 1 ] && [ -z "$out" ] && [ "$(cat "$tmp/ck.err")" = "no file" ]; then pass "--check-avatar on a missing file: exit 1, stderr [no file]"
else flunk "--check-avatar on a missing file: exit $rc, stdout [$out], stderr [$(cat "$tmp/ck.err")]"; fi

if [ ! -e "$sess" ]; then pass "--check-avatar does not start the app (no sessions directory was created)"
else flunk "--check-avatar created $sess: it started the app"; fi

# --- the app --------------------------------------------------------------------------------------------------------
now() { perl -MTime::HiRes=time -e 'printf "%.3f", time'; }
lines() { wc -l <"$1" | tr -d ' '; }
hook() {
  [ "${NOTCH_DIR:-}" = "$tmp/sessions" ] || { echo "FAIL hook() would write outside the test directory"; exit 1; }
  printf '%s' "$1" | "$root/hooks/notch-hook.sh"
}
ev() { hook "{\"session_id\":\"$1\",\"cwd\":\"/x/$1\",\"hook_event_name\":\"$2\"${3:-}}"; }
stamp() { perl -MTime::HiRes=time -ne 'BEGIN { $| = 1 } printf "%.3f %s", time, $_'; }
stop_app() { [ -n "$app" ] && kill "$app" 2>/dev/null; wait "$app" 2>/dev/null; app=""; }

skip_env=0
# start_app <log> <avatar path> <trace: 1 or 0>: starts the app; waits for its `avatar:` line or for "no notched display".
start_app() {
  local log="$1" path="$2" trace="$3" i=0
  : >"$log"
  if [ "$trace" = 1 ]; then
    NOTCH_DIR="$tmp/sessions" NOTCH_TRACE=1 NOTCH_AVATAR="$path" NOTCH_MEDIA_ADAPTER="$adapter" "$bin" 2> >(stamp >"$log") & app=$!
  else
    NOTCH_DIR="$tmp/sessions" NOTCH_AVATAR="$path" NOTCH_MEDIA_ADAPTER="$adapter" "$bin" 2> >(stamp >"$log") & app=$!
  fi
  while [ "$i" -lt 100 ]; do
    if grep -qF 'no notched display' "$log"; then skip_env=1; stop_app; return 1; fi
    grep -qF ' avatar: ' "$log" && return 0
    kill -0 "$app" 2>/dev/null || { sleep 0.3; grep -qF 'no notched display' "$log" && { skip_env=1; stop_app; return 1; }; app=""; return 1; }
    sleep 0.1; i=$((i + 1))
  done
  stop_app
  return 1
}

adapter="$root/src-tauri/target/adapter"
if [ ! -d "$adapter" ]; then "$root/scripts/build-adapter.sh" >/dev/null 2>&1 || echo "NOTE adapter build failed: Now Playing falls back to AppleScript"; fi

mkdir -p "$tmp/sessions"

# 1. a missing file, without NOTCH_TRACE.
missing="$tmp/nothing-here.avatar.json"
if start_app "$tmp/a1.log" "$missing" 0; then
  line="$(grep -F ' avatar: ' "$tmp/a1.log" | head -n 1 | cut -d' ' -f2-)"
  if [ "$line" = "avatar: $missing: no file, using the default" ]; then pass "a missing file logs [$line] with no NOTCH_TRACE"
  else flunk "a missing file: logged [$line]"; fi
  stop_app
elif [ "$skip_env" = 0 ]; then flunk "a missing file: no avatar: line on stderr"; cat "$tmp/a1.log"; fi

# 2. an invalid file.
if [ "$skip_env" = 0 ]; then
  cp "$tmp/bad.json" "$tmp/bad.avatar.json"
  if start_app "$tmp/a2.log" "$tmp/bad.avatar.json" 1; then
    line="$(grep -F ' avatar: ' "$tmp/a2.log" | head -n 1 | cut -d' ' -f2-)"
    case "$line" in
      "avatar: $tmp/bad.avatar.json: /animations/idle/steps/0/expression: "*", using the default") pass "an invalid file logs [$line]" ;;
      *) flunk "an invalid file: logged [$line]" ;;
    esac
    stop_app
  elif [ "$skip_env" = 0 ]; then flunk "an invalid file: no avatar: line on stderr"; cat "$tmp/a2.log"; fi
fi

# 3. a valid file, and the moods.
applog="$tmp/a3.log"
# wait_mood <mood> <secs> <after line>: waits for a traced `avatar: <mood>` line after the line number; sets $hit_ts.
hit_ts=""
wait_mood() {
  local want="$1" secs="$2" after="$3" i=0 line
  while [ "$i" -lt $((secs * 50)) ]; do
    line="$(tail -n +$((after + 1)) "$applog" | grep -E " avatar: $want\$" | head -n 1)"
    if [ -n "$line" ]; then hit_ts="${line%% *}"; return 0; fi
    sleep 0.02; i=$((i + 1))
  done
  return 1
}
# playing: whether this app's latest `media:` trace line says a track is playing.
playing() { grep -F ' media: ' "$applog" | tail -n 1 | grep -qF ' playing "'; }

if [ "$skip_env" = 0 ]; then
  cp "$fixture" "$tmp/ok.avatar.json"
  if start_app "$applog" "$tmp/ok.avatar.json" 1; then
    line="$(grep -F ' avatar: ' "$applog" | head -n 1 | cut -d' ' -f2-)"
    if [ "$line" = "avatar: loaded $tmp/ok.avatar.json" ]; then pass "a valid file logs [$line]"; else flunk "a valid file: logged [$line]"; fi
    if grep -qF 'using the default' "$applog"; then flunk "a valid file also logged 'using the default'"; fi

    i=0; loaded=0
    while [ "$i" -lt 150 ]; do grep -qF 'page: loaded' "$applog" && { loaded=1; break; }; sleep 0.1; i=$((i + 1)); done
    [ "$loaded" = 1 ] && pass "page loaded" || { flunk "page loads"; cat "$applog"; exit 1; }
    sleep 3

    m="$(lines "$applog")"
    ev w1 PreToolUse ',"tool_name":"Read","tool_input":{}'
    if wait_mood working 3 "$m"; then pass "a working session traces avatar: working"; else flunk "no 'avatar: working' within 3 s of a PreToolUse"; fi
    sleep 0.5

    m="$(lines "$applog")"
    ev w1 PermissionRequest ',"tool_name":"Bash","tool_input":{}'
    if wait_mood alert 3 "$m"; then pass "a PermissionRequest traces avatar: alert"; else flunk "no 'avatar: alert' within 3 s of a PermissionRequest"; fi
    sleep 0.5

    m="$(lines "$applog")"
    ev w1 Stop
    if wait_mood celebrate 3 "$m"; then
      pass "Stop (no other session working) traces avatar: celebrate"
      celebrate_ts="$hit_ts"
      i=0; got=0
      while [ "$i" -lt 150 ]; do
        line="$(tail -n +$((m + 1)) "$applog" | grep -E " avatar: (idle|listening)\$" | head -n 1)"
        if [ -n "$line" ]; then got=1; quiet_line="$line"; break; fi
        sleep 0.04; i=$((i + 1))
      done
      if [ "$got" = 1 ]; then
        quiet_ts="${quiet_line%% *}"; quiet_mood="${quiet_line##* }"
        playing && want=listening || want=idle
        delay="$(awk -v a="$celebrate_ts" -v b="$quiet_ts" 'BEGIN { printf "%.2f", b - a }')"
        if [ "$quiet_mood" = "$want" ]; then pass "after the celebration the mood is $quiet_mood (Now Playing says $want)"
        else flunk "after the celebration the mood is $quiet_mood, but the latest media: line says $want"; fi
        if awk -v d="$delay" 'BEGIN { exit !(d >= 3.7 && d <= 4.6) }'; then pass "the celebration lasted ${delay} s (4 s, within 3.7 to 4.6)"
        else flunk "the celebration lasted ${delay} s (want about 4)"; fi
      else flunk "no avatar: idle or listening within 6 s of the celebration"; fi
    else flunk "no 'avatar: celebrate' within 3 s of a Stop"; fi
    sleep 0.5

    m="$(lines "$applog")"
    ev e1 StopFailure
    if wait_mood error 3 "$m"; then pass "a StopFailure traces avatar: error"; else flunk "no 'avatar: error' within 3 s of a StopFailure"; fi
    sleep 0.3

    # A mood is traced when it changes: the same mood never twice in a row.
    grep -E ' avatar: (alert|error|celebrate|working|listening|idle|drowsy|sleeping)$' "$applog" | awk '{print $3}' >"$tmp/moods.txt"
    if [ -n "$(uniq -d "$tmp/moods.txt" | head -n 1)" ]; then
      flunk "a mood was traced twice in a row: $(tr '\n' ' ' <"$tmp/moods.txt")"
    else pass "no mood is traced twice in a row ($(tr '\n' ' ' <"$tmp/moods.txt"))"; fi
    stop_app
  elif [ "$skip_env" = 0 ]; then flunk "a valid file: no avatar: line on stderr"; cat "$applog"; fi
fi

if [ "$fail" -ne 0 ]; then echo "--- app log"; cat "$applog" 2>/dev/null; exit 1; fi
if [ "$skip_env" = 1 ]; then echo "SKIP no notched display: the app checks did not run"; exit 2; fi
exit 0
