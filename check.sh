#!/bin/bash
# Builds and launches the spike, then asserts the brief's criteria against the live window server.
set -u
cd "$(dirname "$0")"
export PATH="/opt/homebrew/opt/rustup/bin:$PATH"
(cd src-tauri && cargo build --release -q) || exit 2
swiftc -O check.swift -o check-bin || exit 3
./src-tauri/target/release/notch-spike & app=$!
trap 'kill "$app" 2>/dev/null' EXIT
sleep 2
./check-bin "$app"; code=$?
exit $code
