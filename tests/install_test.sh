#!/bin/bash
# Behavior tests for scripts/install-hooks.sh and uninstall-hooks.sh (CONTRACT.md sections 1 and 5).
# Only ever touches a temp settings.json.
HERE="$(cd "$(dirname "$0")" && pwd)"
REPO="$(cd "$HERE/.." && pwd)"
INSTALL="$REPO/scripts/install-hooks.sh"
UNINSTALL="$REPO/scripts/uninstall-hooks.sh"
HOOK_SUFFIX="hooks/notch-hook.sh"
ROOT="$(mktemp -d)"
trap 'rm -rf "$ROOT"' EXIT

EVENTS="SessionStart UserPromptSubmit PreToolUse PostToolUse PostToolUseFailure PermissionRequest Notification Stop StopFailure SessionEnd"
FAILS=0
pass() { echo "PASS: $1"; }
fail() { echo "FAIL: $1 -- $2"; FAILS=$((FAILS + 1)); }

# Number of hook commands ending in notch-hook.sh under event $2 in file $1.
ours() { jq --arg ev "$2" --arg suf "$HOOK_SUFFIX" \
  '[(.hooks[$ev] // [])[] | .hooks[]? | select(.command | endswith($suf))] | length' "$1"; }
total_ours() { jq --arg suf "$HOOK_SUFFIX" \
  '[(.hooks // {})[][] | .hooks[]? | select(.command | endswith($suf))] | length' "$1"; }

fresh() {
  F="$(mktemp -d "$ROOT/s.XXXXXX")/settings.json"
  cat > "$F" <<'JSON'
{
  "model": "opus",
  "hooks": {
    "PreToolUse": [
      {"matcher": "Bash", "hooks": [{"type": "command", "command": "/usr/local/bin/other-pre.sh", "timeout": 5}]}
    ],
    "Stop": [
      {"hooks": [{"type": "command", "command": "/usr/local/bin/other-stop.sh"}]}
    ],
    "SubagentStop": [
      {"hooks": [{"type": "command", "command": "/usr/local/bin/other-substop.sh"}]}
    ]
  }
}
JSON
  ORIG="$(jq -S . "$F")"
}

# -- install once
fresh
bash "$INSTALL" "$F" >/dev/null 2>&1; RC=$?
NAME="install exits 0"
if [ "$RC" -eq 0 ]; then pass "$NAME"; else fail "$NAME" "exit $RC"; fi

NAME="install registers exactly one group per contract event"
BAD=""
for EV in $EVENTS; do
  N="$(ours "$F" "$EV")"
  [ "$N" = "1" ] || BAD="$BAD $EV=$N"
done
if [ -z "$BAD" ]; then pass "$NAME"; else fail "$NAME" "counts:$BAD"; fi

NAME="install registers 10 groups in total"
N="$(total_ours "$F")"
if [ "$N" = "10" ]; then pass "$NAME"; else fail "$NAME" "total $N"; fi

NAME="registered hooks are synchronous with timeout 2 and absolute command"
BAD="$(jq -r --arg suf "$HOOK_SUFFIX" '[.hooks[][] | .hooks[] | select(.command | endswith($suf))
  | select(.type != "command" or .timeout != 2 or (has("async") and .async == true) or (.command | startswith("/") | not))] | length' "$F")"
if [ "$BAD" = "0" ]; then pass "$NAME"; else fail "$NAME" "$BAD bad registrations"; fi

NAME="registered command points at this repo's hook script"
BAD="$(jq -r --arg want "$REPO/$HOOK_SUFFIX" '[.hooks[][] | .hooks[] | select(.command | endswith("notch-hook.sh")) | select(.command | contains($want) | not)] | length' "$F")"
if [ "$BAD" = "0" ]; then pass "$NAME"; else fail "$NAME" "$BAD commands not containing $REPO/$HOOK_SUFFIX"; fi

NAME="install preserves pre-existing hooks and settings"
GOT="$(jq -c '[.model, .hooks.PreToolUse[0].hooks[0].command, .hooks.PreToolUse[0].matcher, .hooks.Stop[0].hooks[0].command, .hooks.SubagentStop[0].hooks[0].command, (.hooks.PreToolUse|length), (.hooks.Stop|length)]' "$F")"
WANT='["opus","/usr/local/bin/other-pre.sh","Bash","/usr/local/bin/other-stop.sh","/usr/local/bin/other-substop.sh",2,2]'
if [ "$GOT" = "$WANT" ]; then pass "$NAME"; else fail "$NAME" "got $GOT"; fi

NAME="install creates a backup of the original"
set -- "$F".notch-backup-*
if [ -f "$1" ] && [ "$(jq -S . "$1")" = "$ORIG" ]; then pass "$NAME"; else fail "$NAME" "backup missing or differs: $*"; fi
case "$1" in
  *.notch-backup-[0-9]*) pass "backup name ends in a unix timestamp" ;;
  *) fail "backup name ends in a unix timestamp" "$1" ;;
esac

# -- idempotent
bash "$INSTALL" "$F" >/dev/null 2>&1; RC=$?
NAME="second install exits 0"
if [ "$RC" -eq 0 ]; then pass "$NAME"; else fail "$NAME" "exit $RC"; fi
NAME="second install leaves one registration per event"
BAD=""
for EV in $EVENTS; do
  N="$(ours "$F" "$EV")"
  [ "$N" = "1" ] || BAD="$BAD $EV=$N"
done
if [ -z "$BAD" ] && [ "$(total_ours "$F")" = "10" ]; then pass "$NAME"; else fail "$NAME" "counts:$BAD total $(total_ours "$F")"; fi
NAME="second install keeps other hooks once"
GOT="$(jq -c '[(.hooks.PreToolUse|length), (.hooks.Stop|length), (.hooks.SubagentStop|length)]' "$F")"
if [ "$GOT" = "[2,2,1]" ]; then pass "$NAME"; else fail "$NAME" "got $GOT"; fi

# -- uninstall
bash "$UNINSTALL" "$F" >/dev/null 2>&1; RC=$?
NAME="uninstall exits 0"
if [ "$RC" -eq 0 ]; then pass "$NAME"; else fail "$NAME" "exit $RC"; fi
NAME="uninstall removes every notch-hook registration"
if [ "$(total_ours "$F")" = "0" ]; then pass "$NAME"; else fail "$NAME" "left $(total_ours "$F")"; fi
NAME="uninstall restores the original hooks exactly"
if [ "$(jq -S . "$F")" = "$ORIG" ]; then pass "$NAME"; else fail "$NAME" "differs: $(jq -c . "$F")"; fi
NAME="uninstall drops event keys left empty"
GOT="$(jq -c '.hooks | keys' "$F")"
if [ "$GOT" = '["PreToolUse","Stop","SubagentStop"]' ]; then pass "$NAME"; else fail "$NAME" "keys $GOT"; fi

# -- uninstall on a settings file with no hooks of ours
NAME="uninstall is a no-op when nothing of ours is present"
bash "$UNINSTALL" "$F" >/dev/null 2>&1; RC=$?
if [ "$RC" -eq 0 ] && [ "$(jq -S . "$F")" = "$ORIG" ]; then pass "$NAME"; else fail "$NAME" "exit $RC, file $(jq -c . "$F")"; fi

# -- install into a settings file with no hooks key
NAME="install into settings without hooks key, then uninstall leaves no events"
F="$(mktemp -d "$ROOT/e.XXXXXX")/settings.json"
echo '{"model":"sonnet"}' > "$F"
bash "$INSTALL" "$F" >/dev/null 2>&1; RC1=$?
N1="$(total_ours "$F")"
bash "$UNINSTALL" "$F" >/dev/null 2>&1; RC2=$?
GOT="$(jq -c '.model, ((.hooks // {}) | keys)' "$F" | tr '\n' ' ')"
if [ "$RC1" -eq 0 ] && [ "$RC2" -eq 0 ] && [ "$N1" = "10" ] && [ "$GOT" = '"sonnet" [] ' ]; then pass "$NAME"
else fail "$NAME" "rc $RC1/$RC2, installed $N1, after: $GOT"; fi

if [ "$FAILS" -eq 0 ]; then echo "ALL PASS"; exit 0; fi
echo "$FAILS FAILED"
exit 1
