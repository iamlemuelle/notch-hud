#!/bin/bash
# Rust's `parse` must accept exactly what avatar-core 0.1.0's `parseAvatarDefinition` accepts (CONTRACT.md section 10,
# "Avatar file"). This installs the pinned package into a temp directory, builds a corpus of definitions (valid
# fixtures and single-rule mutations: one per rule 1 to 3, duplicate keys, the size limit, odd JSON, numeric bounds at
# and one double past each limit; see tests/avatar/schema-compare.mjs), runs the official parser and
# `notch-hud --check-avatar` on each file, and fails when their verdicts (accept or reject) differ. Every
# disagreement is printed.
#
# It needs the network once (npm install) and no display: --check-avatar never starts the app.
# Exit 0 when every verdict agrees, 1 when one differs, 2 when the environment cannot run it (no node or npm, the
# install failed, the build failed).
set -u
unset CLAUDE_CODE_MESSAGING_SOCKET CLAUDE_CODE_MESSAGING_TOKEN  # never let a test reach a real session's inbox
root="$(cd "$(dirname "$0")/.." && pwd)"
export PATH="/opt/homebrew/opt/rustup/bin:$PATH"

command -v node >/dev/null || { echo "SKIP node is not installed"; exit 2; }
command -v npm >/dev/null || { echo "SKIP npm is not installed"; exit 2; }

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

npm install --prefix "$tmp/core" --no-audit --no-fund @bible-strong/avatar-core@0.1.0 >"$tmp/npm.log" 2>&1 || { cat "$tmp/npm.log"; echo "SKIP npm install @bible-strong/avatar-core@0.1.0 failed (no network?)"; exit 2; }
core="$tmp/core/node_modules/@bible-strong/avatar-core/dist/index.js"
[ -f "$core" ] || { echo "SKIP the installed package has no dist/index.js"; exit 2; }
grep -q '"version": "0.1.0"' "$tmp/core/node_modules/@bible-strong/avatar-core/package.json" || { echo "SKIP the installed package is not 0.1.0"; exit 2; }

(cd "$root/src-tauri" && cargo build --release -q) || { echo "SKIP app build failed"; exit 2; }
bin="$root/src-tauri/target/release/notch-hud"
[ -x "$bin" ] || { echo "SKIP no release binary at $bin"; exit 2; }

node "$root/tests/avatar/schema-compare.mjs" "$core" "$bin"
rc=$?
if [ "$rc" -eq 0 ]; then echo "PASS notch-hud and avatar-core agree on every file"; exit 0; fi
if [ "$rc" -eq 2 ]; then echo "SKIP schema-compare.mjs could not run"; exit 2; fi
echo "FAIL notch-hud and avatar-core disagree (see DISAGREE lines)"
exit 1
