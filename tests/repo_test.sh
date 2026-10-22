#!/bin/bash
# Structural rules of the repository (CONTRACT.md section 10), cheap enough for CI (tests/page_test.sh runs it):
#   - no tracked .json file outside src-tauri/avatar/default.avatar.json and tests/avatar/fixtures/ holds an avatar
#     definition (`"schema": "bible-strong/avatar-definition"`, whitespace-insensitive): a user's avatar is never
#     committed;
#   - the committed dist/vendor/avatar-core.js has the sha256 recorded in vendor/VENDORED.md and as BUNDLE_SHA256 in
#     scripts/vendor-avatar-core.sh.
#
# Exit 0 when every check passes, 1 when one fails.
set -u
root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root" || exit 1
fail=0
pass() { echo "PASS $1"; }
flunk() { echo "FAIL $1"; fail=1; }

# 1. no stray avatar definitions among the tracked json files.
bad=""
n=0
while IFS= read -r -d '' f; do
  case "$f" in
    src-tauri/avatar/default.avatar.json | tests/avatar/fixtures/*) continue ;;
  esac
  [ -f "$f" ] || continue
  n=$((n + 1))
  if tr -d ' \t\r\n' <"$f" | grep -qF '"schema":"bible-strong/avatar-definition"'; then bad="$bad $f"; fi
done < <(git ls-files -z '*.json')
if [ -z "$bad" ]; then pass "no tracked .json file outside the default and the fixtures is an avatar definition ($n checked)"
else flunk "avatar definition(s) committed outside the allowed places:$bad"; fi

# 2. the vendored renderer matches its recorded hash.
bundle=dist/vendor/avatar-core.js
if [ ! -f "$bundle" ]; then flunk "$bundle is missing"
elif ! git ls-files --error-unmatch "$bundle" >/dev/null 2>&1; then flunk "$bundle is not tracked"
else
  actual="$(shasum -a 256 "$bundle" | cut -d' ' -f1)"
  doc="$(grep -F 'dist/vendor/avatar-core.js' vendor/VENDORED.md | grep -o 'bundle sha256 `[0-9a-f]\{64\}`' | grep -o '[0-9a-f]\{64\}')"
  script="$(sed -n 's/^BUNDLE_SHA256=\([0-9a-f]\{64\}\)$/\1/p' scripts/vendor-avatar-core.sh)"
  [ -n "$doc" ] || flunk "vendor/VENDORED.md has no bundle sha256 for $bundle"
  [ -n "$script" ] || flunk "scripts/vendor-avatar-core.sh has no BUNDLE_SHA256"
  if [ "$actual" = "$doc" ]; then pass "$bundle sha256 equals the one in vendor/VENDORED.md ($actual)"; else flunk "$bundle sha256 is $actual, vendor/VENDORED.md says [$doc]"; fi
  if [ "$actual" = "$script" ]; then pass "$bundle sha256 equals BUNDLE_SHA256 in scripts/vendor-avatar-core.sh"; else flunk "$bundle sha256 is $actual, BUNDLE_SHA256 is [$script]"; fi
fi
exit "$fail"
