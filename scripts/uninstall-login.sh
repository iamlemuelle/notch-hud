#!/bin/sh
# Stops the notch panel's LaunchAgent and removes it. Safe to run when nothing is installed.
# Usage: uninstall-login.sh [plist]
set -u
label=com.lem.notch-hud
plist="${1:-$HOME/Library/LaunchAgents/$label.plist}"
launchctl bootout "gui/$(id -u)/$label" 2>/dev/null || true
rm -f "$plist"
