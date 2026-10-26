#!/bin/bash
# The test driver against an ordinary app: `drive paste` puts a string on the pasteboard and posts Command-V into an
# empty TextEdit document, and the document must then hold that string. It guards the driver's Command chords, which
# prompt_test.sh relies on for Command-V, -H, -W and -Q: with the event's character set by hand, Command-V pasted
# nothing (measured 2026-10-26), so a check built on them could pass without the keys ever acting.
#
# It posts key events, so the terminal running it needs Accessibility. Exit 0 when the paste lands, 1 when it does
# not, 2 when the environment cannot run it. Do not touch the keyboard or switch apps while it runs. It opens a temp
# document in TextEdit, closes it at the end, and quits TextEdit only if it was not running before.
set -u
root="$(cd "$(dirname "$0")/.." && pwd)"
tmp="$(mktemp -d)"
drive="$tmp/drive"
name="notch-drive-check-$$.txt"
doc="$tmp/$name"
te=""; te_started=0
cleanup() {
  if [ -n "$te" ]; then
    "$drive" axclose "$te" "$name" 2>/dev/null
    [ "$te_started" -eq 1 ] && "$drive" quit "$te" 2>/dev/null
  fi
}
trap cleanup EXIT

swiftc -O "$root/tests/controls/Drive.swift" -o "$drive" 2>"$tmp/swiftc.log" || { cat "$tmp/swiftc.log"; echo "FAIL driver build"; exit 2; }
"$drive" access || {
  echo "SKIP needs Accessibility to post events: run from a terminal that has it (the Claude app's Terminal panel)"
  exit 2
}

pgrep -xq TextEdit || te_started=1
: >"$doc"
open -a TextEdit "$doc" || { echo "SKIP cannot open TextEdit"; exit 2; }
for _ in $(seq 1 50); do
  f="$("$drive" front)"
  if [ "${f#* }" = "com.apple.TextEdit" ]; then te="${f%% *}"; "$drive" axtext "$te" "$name" >/dev/null 2>&1 && break; fi
  sleep 0.2
done
[ -n "$te" ] || { echo "SKIP TextEdit did not come up frontmost with the document"; exit 2; }

out="$("$drive" paste "pasted-text" 2>&1)" || { echo "FAIL drive paste: $out"; exit 1; }
sleep 0.3
text="$("$drive" axtext "$te" "$name" 2>&1)"
if [ "$text" = "pasted-text" ]; then
  echo "PASS Command-V from the driver pasted into TextEdit: [$text]"
  exit 0
fi
echo "FAIL Command-V from the driver: the document holds [$text], want [pasted-text]"
exit 1
