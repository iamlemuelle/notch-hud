#!/bin/bash
# Builds NowPlayingFixture.app into src-tauri/target/fixture/. It has to be an .app: MediaRemote only reports
# players that have a bundle identifier.
set -euo pipefail

root="$(cd "$(dirname "$0")/../.." && pwd)"
app="$root/src-tauri/target/fixture/NowPlayingFixture.app"

rm -rf "$app"
mkdir -p "$app/Contents/MacOS"
swiftc -O -o "$app/Contents/MacOS/NowPlayingFixture" "$root/tests/media/NowPlayingFixture.swift"
cat > "$app/Contents/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleIdentifier</key><string>dev.lem.notchhud.fixture</string>
  <key>CFBundleName</key><string>NowPlayingFixture</string>
  <key>CFBundleExecutable</key><string>NowPlayingFixture</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>LSUIElement</key><true/>
</dict>
</plist>
PLIST
codesign --force --sign - "$app" 2>/dev/null
echo "$app"
