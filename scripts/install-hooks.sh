#!/bin/sh
# Registers hooks/notch-hook.sh for every event the notch app reads. Idempotent.
# Usage: install-hooks.sh [settings.json]
set -eu
root=$(cd "$(dirname "$0")/.." && pwd)
hook="$root/hooks/notch-hook.sh"
file="${1:-$HOME/.claude/settings.json}"
events="SessionStart UserPromptSubmit PreToolUse PostToolUse PostToolUseFailure PermissionRequest Notification Stop StopFailure SessionEnd"

[ -f "$file" ] || { mkdir -p "$(dirname "$file")"; echo '{}' >"$file"; }
cp "$file" "$file.notch-backup-$(date +%s)"
tmp=$(mktemp "$file.XXXXXX")
/usr/bin/jq --arg hook "$hook" --arg events "$events" '
  def clean: map(select([.hooks[]? | .command] | index($hook) | not));
  .hooks = ((.hooks // {}) | with_entries(.value |= clean))
  | reduce ($events | split(" ")[]) as $e (.;
      .hooks[$e] = ((.hooks[$e] // []) + [{hooks: [{type: "command", command: $hook, timeout: 2}]}]))
' "$file" >"$tmp"
mv "$tmp" "$file"
