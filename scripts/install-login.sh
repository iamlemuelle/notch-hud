#!/bin/sh
# Starts the notch panel at login through a LaunchAgent, and loads it now. Idempotent.
# Usage: install-login.sh [plist]
set -eu
root=$(cd "$(dirname "$0")/.." && pwd)
label=com.lem.notch-hud
plist="${1:-$HOME/Library/LaunchAgents/$label.plist}"
bin="${NOTCH_HUD_BIN:-$root/src-tauri/target/release/notch-hud}"
case "$bin" in /*) ;; *) bin="$PWD/$bin" ;; esac

if [ ! -f "$bin" ] || [ ! -x "$bin" ]; then
  echo "notch-hud: build first: cd src-tauri && cargo build --release" >&2
  exit 1
fi
case "$bin" in *'&'* | *'<'* | *'>'*)
  echo "notch-hud: binary path has a character the plist cannot hold (&, < or >): $bin" >&2
  exit 1 ;;
esac

mkdir -p "$(dirname "$plist")"
tmp=$(mktemp "$plist.XXXXXX")
trap 'rm -f "$tmp"' EXIT
cat >"$tmp" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key>
  <string>$label</string>
  <key>ProgramArguments</key>
  <array>
    <string>$bin</string>
  </array>
  <key>RunAtLoad</key>
  <true/>
  <!-- Restart after a crash only; a clean exit (no notched display) stays stopped. -->
  <key>KeepAlive</key>
  <dict>
    <key>SuccessfulExit</key>
    <false/>
  </dict>
  <key>ProcessType</key>
  <string>Interactive</string>
  <key>StandardErrorPath</key>
  <string>$HOME/Library/Logs/notch-hud.log</string>
</dict>
</plist>
PLIST
/usr/bin/plutil -lint -s "$tmp"
chmod 644 "$tmp"
mv "$tmp" "$plist"

uid=$(id -u)
launchctl bootout "gui/$uid/$label" 2>/dev/null || true
# bootout returns before the old service is gone, and bootstrapping over it fails with exit 5.
deadline=$(($(date +%s) + 5))
while launchctl print "gui/$uid/$label" >/dev/null 2>&1; do
  if [ "$(date +%s)" -ge "$deadline" ]; then
    echo "notch-hud: old login item did not stop" >&2
    exit 1
  fi
  sleep 0.1
done
launchctl bootstrap "gui/$uid" "$plist"
echo "notch-hud: login item installed: $plist"
