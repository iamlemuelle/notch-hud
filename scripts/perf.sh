#!/bin/bash
# Samples a running notch-hud for PERF_SECS seconds (default 60) with top and reports its idle wake-ups per second and
# average CPU (docs/PLAN.md section 7). Leave the panel collapsed and the cursor off the notch while it runs.
#
# Usage: scripts/perf.sh [pid]. Without a pid it samples the one process named notch-hud, which is normally the login
# item. WebKit's WebContent process is not counted: it runs under launchd, not as the app's child.
#
# Exit 0 when the wake-up rate is within PERF_MAX_IDLEW (default 5) per second, 1 when it is over, 2 when there is no
# process to sample.
set -u
secs="${PERF_SECS:-60}"
max="${PERF_MAX_IDLEW:-5}"
pid="${1:-}"
if [ -z "$pid" ]; then
  pids="$(pgrep -x notch-hud)"
  [ -n "$pids" ] || { echo "no notch-hud process running"; exit 2; }
  [ "$(printf '%s\n' "$pids" | wc -l | tr -d ' ')" = 1 ] || { echo "more than one notch-hud; pass a pid: $(echo $pids)"; exit 2; }
  pid="$pids"
fi
kill -0 "$pid" 2>/dev/null || { echo "no process $pid"; exit 2; }

# IDLEW is cumulative since the process started (top marks a changed value with a trailing +), so the rate is the
# difference between the first and the last sample. %CPU of the first sample is not an interval measure; it is skipped.
top -l $((secs + 1)) -s 1 -pid "$pid" -stats pid,idlew,cpu | awk -v pid="$pid" -v secs="$secs" -v max="$max" '
  $1 == pid {
    gsub(/\+/, "", $2)
    if (n == 0) first = $2; else cpu += $3
    last = $2; n++
  }
  END {
    if (n < 2) { print "top returned no samples for pid " pid; exit 2 }
    rate = (last - first) / (n - 1)
    printf "pid %s: %d idle wake-ups in %d s = %.2f/s (budget %s/s), average CPU %.2f%%\n", pid, last - first, n - 1, rate, max, cpu / (n - 1)
    exit !(rate <= max)
  }'
