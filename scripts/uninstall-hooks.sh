#!/bin/sh
# Removes the hook groups installed by install-hooks.sh. Usage: uninstall-hooks.sh [settings.json]
set -eu
root=$(cd "$(dirname "$0")/.." && pwd)
hook="$root/hooks/notch-hook.sh"
file="${1:-$HOME/.claude/settings.json}"

[ -f "$file" ] || exit 0
cp "$file" "$file.notch-backup-$(date +%s)"
tmp=$(mktemp "$file.XXXXXX")
/usr/bin/jq --arg hook "$hook" '
  def clean: map(select([.hooks[]? | .command] | index($hook) | not));
  if .hooks then
    .hooks |= (with_entries(.value |= clean) | with_entries(select(.value | length > 0)))
  else . end
' "$file" >"$tmp"
mv "$tmp" "$file"
