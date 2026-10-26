#!/bin/bash
# Behavior tests for scripts/install-hooks.sh and uninstall-hooks.sh (CONTRACT.md sections 1 and 5).
# Only ever touches a temp settings.json.
unset CLAUDE_CODE_MESSAGING_SOCKET CLAUDE_CODE_MESSAGING_TOKEN  # never let a test reach a real session's inbox
HERE="$(cd "$(dirname "$0")" && pwd)"
REPO="$(cd "$HERE/.." && pwd)"
INSTALL="$REPO/scripts/install-hooks.sh"
UNINSTALL="$REPO/scripts/uninstall-hooks.sh"
HOOK_SUFFIX="hooks/notch-hook.sh"
ROOT="$(mktemp -d)"
trap 'rm -rf "$ROOT"' EXIT

EVENTS="SessionStart UserPromptSubmit PreToolUse PostToolUse PostToolUseFailure PermissionRequest PermissionDenied Notification Stop StopFailure SubagentStop SessionEnd"
FAILS=0
pass() { echo "PASS: $1"; }
fail() { echo "FAIL: $1 -- $2"; FAILS=$((FAILS + 1)); }

# Number of hook commands ending in notch-hook.sh under event $2 in file $1.
ours() { jq --arg ev "$2" --arg suf "$HOOK_SUFFIX" \
  '[(.hooks[$ev] // [])[] | .hooks[]? | select(.command | ltrimstr("\u0027") | rtrimstr("\u0027") | endswith($suf))] | length' "$1"; }
total_ours() { jq --arg suf "$HOOK_SUFFIX" \
  '[(.hooks // {})[][] | .hooks[]? | select(.command | ltrimstr("\u0027") | rtrimstr("\u0027") | endswith($suf))] | length' "$1"; }

# Octal permission bits of $1.
mode_of() { stat -f %Lp "$1" 2>/dev/null || stat -c %a "$1"; }

# Sorted names in the directory of $1, one line, excluding nothing.
dir_line() { ls -A "$(dirname "$1")" | sort | tr '\n' ' ' | sed 's/ $//'; }

# Names in the directory of $1 that are neither the file itself nor a notch-backup.
strays() {
  local base; base="$(basename "$1")"
  ls -A "$(dirname "$1")" | grep -v -e "^$base\$" -e "^$base\.notch-backup-" | tr '\n' ' ' | sed 's/ $//'
}

# Backups of $1, one per line.
backups() { ls -A "$(dirname "$1")" | grep "^$(basename "$1")\.notch-backup-" | sort; }

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

NAME="install registers 12 groups in total"
N="$(total_ours "$F")"
if [ "$N" = "12" ]; then pass "$NAME"; else fail "$NAME" "total $N"; fi

NAME="registered hooks are synchronous with timeout 2 and absolute command"
BAD="$(jq -r --arg suf "$HOOK_SUFFIX" '[.hooks[][] | .hooks[] | select(.command | ltrimstr("\u0027") | rtrimstr("\u0027") | endswith($suf))
  | select(.type != "command" or .timeout != 2 or (has("async") and .async == true) or (.command | ltrimstr("\u0027") | rtrimstr("\u0027") | startswith("/") | not))] | length' "$F")"
if [ "$BAD" = "0" ]; then pass "$NAME"; else fail "$NAME" "$BAD bad registrations"; fi

NAME="registered command points at this repo's hook script"
BAD="$(jq -r --arg want "$REPO/$HOOK_SUFFIX" '[.hooks[][] | .hooks[] | select(.command | ltrimstr("\u0027") | rtrimstr("\u0027") | endswith("notch-hook.sh")) | select(.command | contains($want) | not)] | length' "$F")"
if [ "$BAD" = "0" ]; then pass "$NAME"; else fail "$NAME" "$BAD commands not containing $REPO/$HOOK_SUFFIX"; fi

NAME="install preserves pre-existing hooks and settings"
GOT="$(jq -c '[.model, .hooks.PreToolUse[0].hooks[0].command, .hooks.PreToolUse[0].matcher, .hooks.Stop[0].hooks[0].command, .hooks.SubagentStop[0].hooks[0].command, (.hooks.PreToolUse|length), (.hooks.Stop|length), (.hooks.SubagentStop|length)]' "$F")"
WANT='["opus","/usr/local/bin/other-pre.sh","Bash","/usr/local/bin/other-stop.sh","/usr/local/bin/other-substop.sh",2,2,2]'
if [ "$GOT" = "$WANT" ]; then pass "$NAME"; else fail "$NAME" "got $GOT"; fi

NAME="install creates a backup of the original"
set -- "$F".notch-backup-*
if [ -f "$1" ] && [ "$(jq -S . "$1")" = "$ORIG" ]; then pass "$NAME"; else fail "$NAME" "backup missing or differs: $*"; fi
case "$1" in
  *.notch-backup-[0-9]*.?*) pass "backup name is <file>.notch-backup-<unix ts>.<random>" ;;
  *) fail "backup name is <file>.notch-backup-<unix ts>.<random>" "$1" ;;
esac

NAME="install leaves no file besides settings.json and its backup"
S="$(strays "$F")"
if [ -z "$S" ]; then pass "$NAME"; else fail "$NAME" "stray files: $S"; fi

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
if [ -z "$BAD" ] && [ "$(total_ours "$F")" = "12" ]; then pass "$NAME"; else fail "$NAME" "counts:$BAD total $(total_ours "$F")"; fi
NAME="second install keeps other hooks once"
GOT="$(jq -c '[(.hooks.PreToolUse|length), (.hooks.Stop|length), (.hooks.SubagentStop|length)]' "$F")"
if [ "$GOT" = "[2,2,2]" ]; then pass "$NAME"; else fail "$NAME" "got $GOT"; fi

# -- backups never overwrite each other, even within one second
NAME="two installs in the same second leave two distinct backups"
fresh
bash "$INSTALL" "$F" >/dev/null 2>&1
bash "$INSTALL" "$F" >/dev/null 2>&1
bash "$INSTALL" "$F" >/dev/null 2>&1
N="$(backups "$F" | wc -l | tr -d ' ')"
if [ "$N" = "3" ]; then pass "$NAME"; else fail "$NAME" "backups: $(backups "$F" | tr '\n' ' ')"; fi
NAME="the first backup still holds the original settings"
FIRST_OK=0
for B in $(backups "$F"); do
  [ "$(jq -S . "$(dirname "$F")/$B")" = "$ORIG" ] && FIRST_OK=1
done
if [ "$FIRST_OK" = "1" ]; then pass "$NAME"; else fail "$NAME" "no backup equals the original"; fi

# -- uninstall
fresh
bash "$INSTALL" "$F" >/dev/null 2>&1
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
NAME="uninstall leaves no file besides settings.json and backups"
S="$(strays "$F")"
if [ -z "$S" ]; then pass "$NAME"; else fail "$NAME" "stray files: $S"; fi

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
if [ "$RC1" -eq 0 ] && [ "$RC2" -eq 0 ] && [ "$N1" = "12" ] && [ "$GOT" = '"sonnet" [] ' ]; then pass "$NAME"
else fail "$NAME" "rc $RC1/$RC2, installed $N1, after: $GOT"; fi

# -- symlinked settings: the link survives and the target is updated
for SCRIPT in install uninstall; do
  CASE="$(mktemp -d "$ROOT/l.XXXXXX")"
  mkdir -p "$CASE/real" "$CASE/home"
  fresh
  cp "$F" "$CASE/real/settings.json"
  ln -s "$CASE/real/settings.json" "$CASE/home/settings.json"
  LINK="$CASE/home/settings.json"
  TARGET="$CASE/real/settings.json"
  if [ "$SCRIPT" = "uninstall" ]; then
    bash "$INSTALL" "$TARGET" >/dev/null 2>&1
    bash "$UNINSTALL" "$LINK" >/dev/null 2>&1; RC=$?
  else
    bash "$INSTALL" "$LINK" >/dev/null 2>&1; RC=$?
  fi
  NAME="$SCRIPT through a symlink exits 0 and keeps it a symlink to the same target"
  if [ "$RC" -eq 0 ] && [ -L "$LINK" ] && [ "$(readlink "$LINK")" = "$TARGET" ]; then pass "$NAME"
  else fail "$NAME" "rc $RC, symlink? $([ -L "$LINK" ] && echo yes || echo no), -> $(readlink "$LINK")"; fi
  NAME="$SCRIPT through a symlink updates the target file"
  if [ "$SCRIPT" = "install" ]; then WANT_N=12; else WANT_N=0; fi
  if [ "$(total_ours "$TARGET")" = "$WANT_N" ]; then pass "$NAME"; else fail "$NAME" "target has $(total_ours "$TARGET") of ours, want $WANT_N"; fi
  NAME="$SCRIPT through a symlink leaves no temp file beside the link or the target"
  S="$(strays "$LINK") $(strays "$TARGET")"
  if [ -z "${S// /}" ]; then pass "$NAME"; else fail "$NAME" "stray files: $S"; fi
done

# -- permission bits are preserved
for SCRIPT in install uninstall; do
  fresh
  chmod 640 "$F"
  if [ "$SCRIPT" = "uninstall" ]; then bash "$INSTALL" "$F" >/dev/null 2>&1; chmod 640 "$F"; fi
  if [ "$SCRIPT" = "install" ]; then bash "$INSTALL" "$F" >/dev/null 2>&1; else bash "$UNINSTALL" "$F" >/dev/null 2>&1; fi
  NAME="$SCRIPT keeps settings.json permission bits (640)"
  M="$(mode_of "$F")"
  if [ "$M" = "640" ]; then pass "$NAME"; else fail "$NAME" "mode is $M"; fi
done

# -- a jq failure leaves the settings byte-identical and no temp file behind
for SCRIPT in install uninstall; do
  for BADCONTENT in '{not json' '{"hooks":"oops"}'; do
    F="$(mktemp -d "$ROOT/b.XXXXXX")/settings.json"
    printf '%s' "$BADCONTENT" > "$F"
    cp "$F" "$F.copy-for-compare"
    if [ "$SCRIPT" = "install" ]; then bash "$INSTALL" "$F" >/dev/null 2>&1; else bash "$UNINSTALL" "$F" >/dev/null 2>&1; fi
    NAME="$SCRIPT on '$BADCONTENT' leaves settings.json byte-identical"
    if cmp -s "$F" "$F.copy-for-compare"; then pass "$NAME"; else fail "$NAME" "file is now: $(cat "$F")"; fi
    NAME="$SCRIPT on '$BADCONTENT' leaves no temp file (only backups allowed)"
    S="$(strays "$F" | tr ' ' '\n' | grep -v '^settings.json.copy-for-compare$' | tr '\n' ' ')"
    if [ -z "$S" ]; then pass "$NAME"; else fail "$NAME" "stray files: $S"; fi
  done
done

if [ "$FAILS" -eq 0 ]; then echo "ALL PASS"; exit 0; fi
echo "$FAILS FAILED"
exit 1
