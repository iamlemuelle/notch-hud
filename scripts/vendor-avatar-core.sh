#!/bin/bash
# Rebuilds dist/vendor/avatar-core.js, the avatar renderer, from the pinned npm release of @bible-strong/avatar-core
# (CONTRACT.md section 10, vendor/VENDORED.md). The page has no bundler, so this makes one classic script that defines
# window.AvatarCore. The package's one dependency, ajv (JSON Schema validation), is swapped for a stub that throws:
# Rust validates every avatar before the page gets it. Rerunning it writes the same bytes; it prints their sha256.
#
# Needs npm and network. To update: change VERSION and INTEGRITY (npm view @bible-strong/avatar-core@<v> dist.integrity),
# rerun, check the page, and update the row in vendor/VENDORED.md.
set -euo pipefail
VERSION=0.1.0
INTEGRITY=sha512-b5WAn7QiPlfjomKN9MvXm+Cirpff/yszZ7CJW+BfZSWnZSl+IUdG8zhaElimZAVcLNMzQ9WKngluhJlO6mNUCg==
ESBUILD=0.28.2
root="$(cd "$(dirname "$0")/.." && pwd)"
out="$root/dist/vendor/avatar-core.js"

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
mkdir -p "$tmp/pkg" "$tmp/ajv-stub/dist"
(cd "$tmp/pkg" && npm pack "@bible-strong/avatar-core@$VERSION" --silent >/dev/null)
tgz="$tmp/pkg/bible-strong-avatar-core-$VERSION.tgz"
got="sha512-$(openssl dgst -sha512 -binary "$tgz" | base64 | tr -d '\n')"
[ "$got" = "$INTEGRITY" ] || { echo "vendor-avatar-core: integrity mismatch: got $got" >&2; exit 1; }
tar -xzf "$tgz" -C "$tmp/pkg"

cat >"$tmp/entry.js" <<'JS'
export {
  playAvatarAnimation,
  advanceAvatarPlayback,
  sampleAvatarFrame,
  renderAvatarFrame,
  renderAvatarDefinition,
} from './pkg/package/dist/index.js';
JS
cat >"$tmp/ajv-stub/dist/2020.js" <<'JS'
// notch-hud: ajv is not bundled. Avatars are validated in Rust (src-tauri/src/avatar.rs) before the page gets them.
export default class {
  compile() {
    return () => {
      throw new Error('avatar-core validation is not bundled in notch-hud');
    };
  }
}
JS
banner="/* @bible-strong/avatar-core $VERSION (AGPL-3.0-only), https://github.com/smontlouis/bible-strong-avatar-lab"
banner="$banner. Bundled by scripts/vendor-avatar-core.sh with esbuild $ESBUILD; ajv replaced by a stub. */"
# Run from the temp dir with relative paths, so the path comments esbuild writes are the same on every run.
(cd "$tmp" && npx --yes "esbuild@$ESBUILD" entry.js --bundle --format=iife --global-name=AvatarCore \
  --alias:ajv=./ajv-stub --target=safari16 --charset=utf8 --log-level=warning "--banner:js=$banner" \
  --outfile=out.js)
mkdir -p "$(dirname "$out")"
cp "$tmp/out.js" "$out"
shasum -a 256 "$out" | awk '{print "sha256 " $1}'
