#!/bin/bash
# Structural rules of the repository (CONTRACT.md section 10), cheap enough for CI (tests/page_test.sh runs it):
#   - no tracked .json file outside src-tauri/avatar/default.avatar.json and tests/avatar/fixtures/ holds an avatar
#     definition (`"schema": "bible-strong/avatar-definition"`, whitespace-insensitive): a user's avatar is never
#     committed;
#   - .gitignore ignores avatar.json and *.avatar.json anywhere in the repo, but not src-tauri/avatar/default.avatar.json
#     or the fixtures under tests/avatar/;
#   - the committed dist/vendor/avatar-core.js has the sha256 recorded in vendor/VENDORED.md.
#
# It needs only git (CI runs it on its own, in its own step).
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

# 2. .gitignore keeps a user's avatar out of the repo, and keeps the committed ones in.
for p in avatar.json some/dir/avatar.json x.avatar.json tests/x.avatar.json src-tauri/avatar/mine.avatar.json deep/er/still/x.avatar.json; do
  if git check-ignore -q "$p"; then pass "git ignores $p"; else flunk "git does not ignore $p"; fi
done
for p in src-tauri/avatar/default.avatar.json tests/avatar/fixtures/ball.json; do
  if git check-ignore -q "$p"; then flunk "git ignores $p (it must be committed)"; else pass "git does not ignore $p"; fi
done

# 3. the vendored renderer matches its recorded hash.
bundle=dist/vendor/avatar-core.js
if [ ! -f "$bundle" ]; then flunk "$bundle is missing"
elif ! git ls-files --error-unmatch "$bundle" >/dev/null 2>&1; then flunk "$bundle is not tracked"
else
  actual="$(shasum -a 256 "$bundle" | cut -d' ' -f1)"
  doc="$(grep -F 'dist/vendor/avatar-core.js' vendor/VENDORED.md | grep -o 'bundle sha256 `[0-9a-f]\{64\}`' | grep -o '[0-9a-f]\{64\}')"
  [ -n "$doc" ] || flunk "vendor/VENDORED.md has no bundle sha256 for $bundle"
  if [ "$actual" = "$doc" ]; then pass "$bundle sha256 equals the one in vendor/VENDORED.md ($actual)"; else flunk "$bundle sha256 is $actual, vendor/VENDORED.md says [$doc]"; fi
fi
exit "$fail"
