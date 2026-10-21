#!/bin/bash
# Builds the vendored mediaremote-adapter and stages what the app runs into src-tauri/target/adapter/.
# The app looks there (next to target/<profile>/notch-hud), or in $NOTCH_MEDIA_ADAPTER when set.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
src="$root/vendor/mediaremote-adapter"
build="$root/src-tauri/target/adapter-build"
out="$root/src-tauri/target/adapter"

command -v cmake >/dev/null || { echo "build-adapter: cmake not found (brew install cmake)" >&2; exit 1; }

cmake -S "$src" -B "$build" -DCMAKE_BUILD_TYPE=Release >/dev/null
cmake --build "$build" --config Release >/dev/null

rm -rf "$out.tmp"
mkdir -p "$out.tmp"
cp "$src/bin/mediaremote-adapter.pl" "$out.tmp/"
cp -R "$build/MediaRemoteAdapter.framework" "$out.tmp/"
cp "$build/MediaRemoteAdapterTestClient" "$out.tmp/"
rm -rf "$out"
mv "$out.tmp" "$out"
echo "$out"
