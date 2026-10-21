#!/bin/sh
# Registers hooks/notch-hook.sh for every event the notch app reads. Idempotent.
# Usage: install-hooks.sh [settings.json]
set -eu
root=$(cd "$(dirname "$0")/.." && pwd)
hook="$root/hooks/notch-hook.sh"
file="${1:-$HOME/.claude/settings.json}"
events="SessionStart UserPromptSubmit PreToolUse PostToolUse PostToolUseFailure PermissionRequest PermissionDenied Notification Stop StopFailure SubagentStop SessionEnd"

# Follow a symlink so the link survives and the real file is rewritten.
if [ -L "$file" ]; then
  file=$(/usr/bin/python3 -c 'import os,sys; print(os.path.realpath(sys.argv[1]))' "$file")
fi
[ -f "$file" ] || { mkdir -p "$(dirname "$file")"; echo '{}' >"$file"; }

tmp=$(mktemp "$file.XXXXXX")
trap 'rm -f "$tmp"' EXIT
/usr/bin/jq --arg hook "$hook" --arg events "$events" '
  def clean: map(select([.hooks[]? | .command] | index($hook | @sh) | not));
  .hooks = ((.hooks // {}) | with_entries(.value |= clean))
  | reduce ($events | split(" ")[]) as $e (.;
      .hooks[$e] = ((.hooks[$e] // []) + [{hooks: [{type: "command", command: ($hook | @sh), timeout: 2}]}]))
' "$file" >"$tmp"
chmod "$(stat -f %Lp "$file")" "$tmp"
backup="$file.notch-backup-$(date +%s).$$-$(od -An -N4 -tx1 /dev/urandom | tr -d ' \n')"
[ ! -e "$backup" ] || { echo "backup exists: $backup" >&2; exit 1; }
cp -p "$file" "$backup"
mv "$tmp" "$file"
