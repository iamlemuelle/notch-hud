#!/bin/bash
# Builds and launches the spike, then asserts the brief's criteria against the live window server.
set -u
unset CLAUDE_CODE_MESSAGING_SOCKET CLAUDE_CODE_MESSAGING_TOKEN  # never let a test reach a real session's inbox
cd "$(dirname "$0")"
export PATH="/opt/homebrew/opt/rustup/bin:$PATH"
(cd src-tauri && cargo build --release -q) || exit 2
swiftc -O check.swift -o check-bin || exit 3
# Seed sessions through the real hook into a throwaway directory, never ~/.claude/notch.
export NOTCH_DIR="$(mktemp -d)/sessions"
hook() { printf '%s' "$1" | ./hooks/notch-hook.sh; }
hook '{"session_id":"s1","cwd":"/x/bianki","hook_event_name":"PermissionRequest","tool_name":"Bash","tool_input":{"command":"ls"}}'
hook '{"session_id":"s2","cwd":"/x/lstack","hook_event_name":"PreToolUse","tool_name":"Read","tool_input":{}}'
hook '{"session_id":"s3","cwd":"/x/patunay","hook_event_name":"Stop","stop_hook_active":false}'
hook '{"session_id":"s4","cwd":"/x/kerf","hook_event_name":"SessionStart","source":"startup"}'
# The trace (`sessions:` and `avatar:` lines) goes to a file that check-bin reads.
trace="$(dirname "$NOTCH_DIR")/trace.log"
NOTCH_TRACE=1 ./src-tauri/target/release/notch-hud 2>"$trace" & app=$!
trap 'kill "$app" 2>/dev/null' EXIT
./check-bin "$app" "$trace"; code=$?
[ "$code" = 0 ] || echo "app trace: $trace"
exit $code
