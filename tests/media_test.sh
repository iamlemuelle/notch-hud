#!/bin/bash
# End to end: the real app reads a fixture player through the real adapter, notices when it stops, and falls back to
# AppleScript when the adapter is missing. Asserts on the app's NOTCH_TRACE lines (CONTRACT.md section 7).
set -u
root="$(cd "$(dirname "$0")/.." && pwd)"
export PATH="/opt/homebrew/opt/rustup/bin:$PATH"
fail=0
pass() { echo "PASS $1"; }
flunk() { echo "FAIL $1"; fail=1; }

"$root/scripts/build-adapter.sh" >/dev/null || { echo "FAIL adapter build"; exit 2; }
fixture_app="$("$root/tests/media/build-fixture.sh")" || { echo "FAIL fixture build"; exit 2; }
fixture="$fixture_app/Contents/MacOS/NowPlayingFixture"
(cd "$root/src-tauri" && cargo build --release -q) || { echo "FAIL app build"; exit 2; }

tmp="$(mktemp -d)"
export NOTCH_DIR="$tmp/sessions"
export NOTCH_TRACE=1
app=""; player=""
# Processes this run started and may have to clean up: pids and process group ids recorded from the app's
# descendants, and unique fake adapter directories (pkill on one of those matches only this run's processes).
own_pids=""; own_pgids=""; fake_dirs=""
my_pgid="$(ps -o pgid= -p $$ | tr -d ' ')"
cleanup() {
  local p g d
  [ -n "$player" ] && kill "$player" 2>/dev/null
  [ -n "$app" ] && kill "$app" 2>/dev/null
  for p in $own_pids; do kill -KILL "$p" 2>/dev/null; done
  for g in $own_pgids; do kill -KILL -- "-$g" 2>/dev/null; done
  for d in $fake_dirs; do pkill -KILL -f -- "$d" 2>/dev/null; done
  wait 2>/dev/null
}
trap cleanup EXIT

# Waits up to $2 seconds for a fixed string to appear in the log file $1, looking only at lines after line $4.
wait_for() {
  local log="$1" secs="$2" want="$3" after="${4:-0}" i=0
  while [ "$i" -lt $((secs * 5)) ]; do
    tail -n +$((after + 1)) "$log" | grep -qF -- "$want" && return 0
    sleep 0.2; i=$((i + 1))
  done
  return 1
}

# Prints the pid $1 and every descendant of it.
descendants() {
  local p
  echo "$1"
  for p in $(pgrep -P "$1" 2>/dev/null); do descendants "$p"; done
}

# Prints the pid of this test's adapter stream process, a descendant of app pid $1.
stream_pid() {
  local p
  for p in $(descendants "$1"); do
    ps -o command= -p "$p" 2>/dev/null | grep -qE "mediaremote-adapter\.pl.* stream( |$)" && { echo "$p"; return 0; }
  done
  return 1
}

# Records the pids and process group ids of app pid $1 and its descendants as this run's own. The script's own
# group is never recorded.
record_own() {
  local p g
  for p in $(descendants "$1"); do
    own_pids="$own_pids $p"
    g="$(ps -o pgid= -p "$p" 2>/dev/null | tr -d ' ')"
    [ -n "$g" ] && [ "$g" != "$my_pgid" ] && own_pgids="$own_pgids $g"
  done
}

# Prints the recorded own pids (except $1) and the members of the recorded groups that are still alive.
own_left() {
  local p g
  for p in $own_pids; do [ "$p" != "$1" ] && kill -0 "$p" 2>/dev/null && ps -o pid=,command= -p "$p"; done
  for g in $own_pgids; do pgrep -g "$g" -l 2>/dev/null; done
}

# Makes a fake adapter directory and prints its path. Its `test` command starts a child in a process group of its
# own, with a command line naming the directory, and then sleeps forever; every other command exits 1.
make_fake_adapter() {
  local d
  d="$(mktemp -d -t notch-media-fake)" || return 1
  fake_dirs="$fake_dirs $d"
  mkdir "$d/MediaRemoteAdapter.framework"
  : >"$d/MediaRemoteAdapterTestClient"
  cat >"$d/mediaremote-adapter.pl" <<'PERL'
use POSIX;
use File::Basename;
my $dir = dirname($0);
exit 1 unless ($ARGV[2] // '') eq 'test';
my $pid = fork();
if (!$pid) {
  POSIX::setpgid(0, 0);
  exec { '/bin/sleep' } "$dir/fake-child", '600';
}
sleep 1 while 1;
PERL
  chmod +x "$d/mediaremote-adapter.pl"
  echo "$d"
}

# Waits up to $2 seconds for no process with the string $1 in its command line. Prints what is left.
wait_gone() {
  local i=0 left=""
  while [ "$i" -lt $(($2 * 5)) ]; do
    left="$(pgrep -fl -- "$1")"
    [ -z "$left" ] && return 0
    sleep 0.2; i=$((i + 1))
  done
  echo "$left"; return 1
}

lines() { wc -l <"$1" | tr -d ' '; }

# 1. Adapter path: a playing fixture shows up and disappears when it quits; a dead stream clears the track and
# recovers; killing the app leaves no adapter process behind.
log1="$tmp/app1.log"
"$root/src-tauri/target/release/notch-hud" 2>"$log1" & app=$!
"$fixture" "E2E Song" "E2E Artist" square 223 74 & player=$!
playing='media: adapter ok playing "E2E Song" (dev.lem.notchhud.fixture)'
if wait_for "$log1" 10 "$playing"; then pass "adapter reports the fixture"; else flunk "adapter reports the fixture"; fi

mark="$(lines "$log1")"
kill "$player"; wait "$player" 2>/dev/null; player=""
if wait_for "$log1" 10 'media: adapter ok nothing' "$mark"; then pass "fixture stop clears the track"; else flunk "fixture stop clears the track"; fi

mark="$(lines "$log1")"
"$fixture" "E2E Song" "E2E Artist" square 223 74 & player=$!
wait_for "$log1" 10 "$playing" "$mark" || echo "note: fixture did not show again before the stream kill"
spid="$(stream_pid "$app")"
mark="$(lines "$log1")"
if [ -n "$spid" ]; then kill -KILL "$spid" 2>/dev/null; else echo "note: no adapter stream process found under app $app"; fi
if [ -n "$spid" ] && wait_for "$log1" 10 'media: adapter failed nothing' "$mark"; then pass "stream death clears the track"; else flunk "stream death clears the track"; fi
if [ -n "$spid" ] && wait_for "$log1" 15 "$playing" "$mark"; then pass "stream restarts and shows the fixture again"; else flunk "stream restarts and shows the fixture again"; fi

record_own "$app"
oldapp="$app"
kill -KILL "$app"; wait "$app" 2>/dev/null; app=""
i=0; left=""
while [ "$i" -lt 25 ]; do
  left="$(own_left "$oldapp")"
  [ -z "$left" ] && break
  sleep 0.2; i=$((i + 1))
done
if [ -z "$left" ]; then pass "no orphan adapter processes after app SIGKILL"; else
  flunk "no orphan adapter processes after app SIGKILL"
  echo "left behind:"; echo "$left"
fi
kill "$player" 2>/dev/null; wait "$player" 2>/dev/null; player=""

# 2. Isolation: no adapter, the app keeps running and falls back to AppleScript.
log2="$tmp/app2.log"
NOTCH_MEDIA_ADAPTER=/nonexistent "$root/src-tauri/target/release/notch-hud" 2>"$log2" & app=$!
if wait_for "$log2" 10 'media: adapter Failed ('; then pass "missing adapter is logged"; else flunk "missing adapter is logged"; fi
if wait_for "$log2" 10 'media: applescript '; then pass "fallback reports a state"; else flunk "fallback reports a state"; fi
if kill -0 "$app" 2>/dev/null; then pass "app still running"; else flunk "app still running"; fi

kill "$app" 2>/dev/null; wait "$app" 2>/dev/null; app=""

# 3. Test timeout: a fake adapter whose test client sits in its own process group never exits. The app gives up
# after 5 s, logs it and stops the adapter, and nothing of the fake adapter stays alive.
fake3="$(make_fake_adapter)"
log3="$tmp/app3.log"
NOTCH_MEDIA_ADAPTER="$fake3" "$root/src-tauri/target/release/notch-hud" 2>"$log3" & app=$!
if wait_for "$log3" 12 'media: adapter Failed (test timed out'; then pass "test timeout is logged"; else flunk "test timeout is logged"; fi
left3="$(wait_gone "$fake3" 8)"
if [ -z "$left3" ]; then pass "test timeout stops the test client"; else
  flunk "test timeout stops the test client"
  echo "left behind:"; echo "$left3"
fi
kill "$app" 2>/dev/null; wait "$app" 2>/dev/null; app=""
pkill -KILL -f -- "$fake3" 2>/dev/null

# 4. App killed during test: the watchdog stops the adapter, including the test client in its own group.
fake4="$(make_fake_adapter)"
log4="$tmp/app4.log"
NOTCH_MEDIA_ADAPTER="$fake4" "$root/src-tauri/target/release/notch-hud" 2>"$log4" & app=$!
i=0
while [ "$i" -lt 15 ]; do pgrep -f -- "$fake4/fake-child" >/dev/null && break; sleep 0.2; i=$((i + 1)); done
if pgrep -f -- "$fake4/fake-child" >/dev/null; then pass "fake test client started"; else flunk "fake test client started"; fi
kill -KILL "$app"; wait "$app" 2>/dev/null; app=""
left4="$(wait_gone "$fake4" 8)"
if [ -z "$left4" ]; then pass "app SIGKILL during test stops the test client"; else
  flunk "app SIGKILL during test stops the test client"
  echo "left behind:"; echo "$left4"
fi
pkill -KILL -f -- "$fake4" 2>/dev/null

if [ "$fail" -ne 0 ]; then
  echo "--- app1.log"; cat "$log1"; echo "--- app2.log"; cat "$log2"
  echo "--- app3.log"; cat "$log3"; echo "--- app4.log"; cat "$log4"
fi
exit "$fail"
