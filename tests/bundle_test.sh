#!/bin/bash
# Local only: runs the real scripts/build-app.sh and checks the real bundle (CONTRACT.md section 9).
# Writes src-tauri/target/notch-hud.app, the bundle the login item runs. Uses the signing identity from
# NOTCH_HUD_SIGN_IDENTITY, or the script's default.
set -u
root="$(cd "$(dirname "$0")/.." && pwd)"
export PATH="/opt/homebrew/opt/rustup/bin:$HOME/.cargo/bin:$PATH"
app="$root/src-tauri/target/notch-hud.app"
identity="${NOTCH_HUD_SIGN_IDENTITY:-notch-hud Code Signing}"
fail=0
pass() { echo "PASS $1"; }
flunk() { echo "FAIL $1"; fail=1; }
skip() { echo "SKIP $1"; }
check_eq() { # name actual expected
  if [ "$2" = "$3" ]; then pass "$1"; else flunk "$1 (expected [$3], got [$2])"; fi
}

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

# Runs build-app.sh with stdout in $tmp/$1.out and stderr in $tmp/$1.err; sets RC to its exit code.
build() { # tag
  "$root/scripts/build-app.sh" >"$tmp/$1.out" 2>"$tmp/$1.err"; RC=$?
}
cdhash() { codesign -dvvv "$app" 2>&1 | grep '^CDHash='; }
requirement() { codesign -d -r- "$app" 2>/dev/null; }

# 1. Build
build one
check_eq "build exits 0" "$RC" "0"
if [ "$RC" -ne 0 ]; then echo "--- build stderr"; cat "$tmp/one.err"; echo "build failed, cannot check the bundle"; exit 1; fi
check_eq "build's last stdout line is the bundle path" "$(tail -n 1 "$tmp/one.out")" "$app"

# 2. Info.plist
plist="$app/Contents/Info.plist"
usage="$(plutil -extract NSAppleEventsUsageDescription raw -o - "$plist" 2>/dev/null)"
if [ -n "$usage" ]; then pass "NSAppleEventsUsageDescription is non-empty"; else flunk "NSAppleEventsUsageDescription is non-empty"; fi
check_eq "CFBundleIdentifier" "$(plutil -extract CFBundleIdentifier raw -o - "$plist" 2>/dev/null)" "dev.lem.notchhud"
check_eq "LSUIElement" "$(plutil -extract LSUIElement raw -o - "$plist" 2>/dev/null)" "true"

# 3. Layout
if [ -x "$app/Contents/MacOS/notch-hud" ]; then pass "Contents/MacOS/notch-hud is executable"; else flunk "Contents/MacOS/notch-hud is executable"; fi
ad="$app/Contents/Resources/adapter"
if [ -f "$ad/mediaremote-adapter.pl" ]; then pass "adapter has mediaremote-adapter.pl"; else flunk "adapter has mediaremote-adapter.pl"; fi
if [ -f "$ad/MediaRemoteAdapterTestClient" ] && [ -x "$ad/MediaRemoteAdapterTestClient" ]; then
  pass "adapter has an executable MediaRemoteAdapterTestClient"; else flunk "adapter has an executable MediaRemoteAdapterTestClient"; fi
if [ -d "$ad/MediaRemoteAdapter.framework" ]; then pass "adapter has MediaRemoteAdapter.framework"; else flunk "adapter has MediaRemoteAdapter.framework"; fi
if [ -L "$ad/MediaRemoteAdapter.framework/Versions/Current" ]; then pass "framework Versions/Current is a symlink"; else flunk "framework Versions/Current is a symlink"; fi

# 4. Signature
if codesign --verify --strict "$app" 2>"$tmp/verify.err"; then pass "codesign --verify --strict"; else
  flunk "codesign --verify --strict"; cat "$tmp/verify.err"
fi
dv="$(codesign -dvv "$app" 2>&1)"
if [ "$identity" = "-" ]; then
  skip "signature is not ad hoc (identity is -)"
  skip "Authority names the identity (identity is -)"
  skip "designated requirement is stable across builds (identity is -)"
else
  if echo "$dv" | grep -qi 'adhoc'; then flunk "signature is not ad hoc"; else pass "signature is not ad hoc"; fi
  if echo "$dv" | grep -qxF "Authority=$identity"; then pass "Authority names the identity"; else flunk "Authority names the identity"; fi

  req1="$(requirement)"; hash1="$(cdhash)"
  build two
  check_eq "second build exits 0" "$RC" "0"
  req2="$(requirement)"; hash2="$(cdhash)"
  if [ -n "$req1" ]; then check_eq "designated requirement is identical across builds" "$req2" "$req1"; else flunk "designated requirement is non-empty"; fi
  echo "note: CDHash first [$hash1] second [$hash2] (may differ)"
fi

# 5. Unknown identity: exits 1, prints the section 9 message, leaves the bundle as it was
before="$(cdhash)"
bad="no such identity $$"
NOTCH_HUD_SIGN_IDENTITY="$bad" "$root/scripts/build-app.sh" >"$tmp/bad.out" 2>"$tmp/bad.err"; RC=$?
check_eq "unknown identity: exit 1" "$RC" "1"
check_eq "unknown identity: stderr" "$(cat "$tmp/bad.err")" "build-app: no code signing identity \"$bad\" (README: Install)"
after="$(cdhash)"
if [ -n "$before" ]; then check_eq "unknown identity: bundle unchanged (CDHash)" "$after" "$before"; else flunk "bundle has a CDHash before the failed build"; fi

exit "$fail"
