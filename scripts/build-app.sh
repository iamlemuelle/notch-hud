#!/bin/bash
# Builds and signs src-tauri/target/notch-hud.app, the bundle the login item runs (CONTRACT.md section 9).
# The signing identity is $NOTCH_HUD_SIGN_IDENTITY, else "notch-hud Code Signing"; "-" signs ad hoc.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
export PATH="/opt/homebrew/opt/rustup/bin:$HOME/.cargo/bin:$PATH"
identity="${NOTCH_HUD_SIGN_IDENTITY:-notch-hud Code Signing}"
target="$root/src-tauri/target"
app="$target/notch-hud.app"

if [ "$identity" = "-" ]; then
  echo "build-app: ad hoc signature, permission grants will not survive a rebuild" >&2
elif ! security find-identity -p codesigning | grep -qF "\"$identity\""; then
  echo "build-app: no code signing identity \"$identity\" (README: Install)" >&2
  exit 1
fi
cargo tauri --version >/dev/null 2>&1 || {
  echo "build-app: tauri-cli not found: cargo install tauri-cli --version 2.12.1 --locked" >&2
  exit 1
}

"$root/scripts/build-adapter.sh" >/dev/null
# This script signs, so Tauri must not: unset the variables that would make it sign with something else.
(cd "$root/src-tauri" && env -u APPLE_SIGNING_IDENTITY -u APPLE_CERTIFICATE cargo tauri build --bundles app >&2)

# Sign a copy, then swap it in by renames, so the login item never starts a half-built or unsigned bundle.
tmp="$target/notch-hud.app.tmp"
old="$target/notch-hud.app.old"
rm -rf "$tmp" "$old"
ditto "$target/release/bundle/macos/notch-hud.app" "$tmp"
sign() { codesign --force --sign "$identity" --timestamp=none "$1"; }
sign "$tmp/Contents/Resources/adapter/MediaRemoteAdapter.framework"
sign "$tmp/Contents/Resources/adapter/MediaRemoteAdapterTestClient"
sign "$tmp"
codesign --verify --strict "$tmp"

if [ -e "$app" ]; then mv "$app" "$old"; fi
mv "$tmp" "$app"
rm -rf "$old"
echo "$app"
