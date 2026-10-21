#!/bin/sh
# Removes the hook groups installed by install-hooks.sh. Usage: uninstall-hooks.sh [settings.json]
set -eu
root=$(cd "$(dirname "$0")/.." && pwd)
hook="$root/hooks/notch-hook.sh"
file="${1:-$HOME/.claude/settings.json}"

if [ -L "$file" ]; then
  file=$(/usr/bin/python3 -c 'import os,sys; print(os.path.realpath(sys.argv[1]))' "$file")
fi
[ -f "$file" ] || exit 0

tmp=$(mktemp "$file.XXXXXX")
trap 'rm -f "$tmp"' EXIT
/usr/bin/jq --arg hook "$hook" '
  def clean: map(select([.hooks[]? | .command] | index($hook | @sh) | not));
  if .hooks then
    .hooks |= (with_entries(.value |= clean) | with_entries(select(.value | length > 0)))
  else . end
' "$file" >"$tmp"
chmod "$(stat -f %Lp "$file")" "$tmp"
backup="$file.notch-backup-$(date +%s).$$-$(od -An -N4 -tx1 /dev/urandom | tr -d ' \n')"
[ ! -e "$backup" ] || { echo "backup exists: $backup" >&2; exit 1; }
cp -p "$file" "$backup"
mv "$tmp" "$file"
