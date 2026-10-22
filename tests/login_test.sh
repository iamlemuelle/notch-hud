#!/bin/bash
# Behavior tests for scripts/install-login.sh and uninstall-login.sh (CONTRACT.md section 6).
# Uses a fake launchctl first on PATH, a temp HOME and a temp plist path. Never touches the real login item.
HERE="$(cd "$(dirname "$0")" && pwd)"
REPO="$(cd "$HERE/.." && pwd)"
INSTALL="$REPO/scripts/install-login.sh"
UNINSTALL="$REPO/scripts/uninstall-login.sh"
ROOT="$(mktemp -d)"
trap 'rm -rf "$ROOT"' EXIT

mkdir -p "$ROOT/fakebin" "$ROOT/home"
export HOME="$ROOT/home"
export LAUNCHCTL_LOG="$ROOT/launchctl.log"
cat > "$ROOT/fakebin/launchctl" <<'FAKE'
#!/bin/bash
echo "$*" >> "$LAUNCHCTL_LOG"
# Models macOS 27.0.1: after bootout the service lingers for FAKE_LINGER successful `print` calls (default 0:
# gone at once), and bootstrap exits 5 while it is still there. State lives in $LAUNCHCTL_LOG.linger.
STATE="$LAUNCHCTL_LOG.linger"
remaining() { cat "$STATE" 2>/dev/null || echo 0; }
case "$1" in
  bootout) echo "${FAKE_LINGER:-0}" > "$STATE"; exit "${FAKE_BOOTOUT_RC:-0}" ;;
  print)
    r="$(remaining)"
    if [ "$r" -gt 0 ]; then echo $((r - 1)) > "$STATE"; exit 0; fi
    exit 113 ;;
  bootstrap)
    if [ "$(remaining)" -gt 0 ]; then echo "Bootstrap failed: 5: Input/output error" >&2; exit 5; fi
    exit "${FAKE_BOOTSTRAP_RC:-0}" ;;
esac
exit 0
FAKE
chmod +x "$ROOT/fakebin/launchctl"
export PATH="$ROOT/fakebin:$PATH"

BIN="$ROOT/bin/notch-hud"
mkdir -p "$ROOT/bin"
printf '#!/bin/sh\nexit 0\n' > "$BIN"
chmod +x "$BIN"
export NOTCH_HUD_BIN="$BIN"
UID_="$(id -u)"
LABEL=com.lem.notch-hud

FAILS=0
pass() { echo "PASS: $1"; }
fail() { echo "FAIL: $1 -- $2"; FAILS=$((FAILS + 1)); }
check_eq() { # name actual expected
  if [ "$2" = "$3" ]; then pass "$1"; else fail "$1" "expected [$3], got [$2]"; fi
}

mode_of() { stat -f %Lp "$1" 2>/dev/null || stat -c %a "$1"; }
extract() { plutil -extract "$2" raw -o - "$1" 2>/dev/null; }
reset() { : > "$LAUNCHCTL_LOG"; rm -f "$LAUNCHCTL_LOG.linger"; unset FAKE_BOOTOUT_RC FAKE_BOOTSTRAP_RC FAKE_LINGER; }
need_script() { # name script
  if [ ! -f "$2" ]; then fail "$1" "missing script $2"; return 1; fi
  return 0
}
# Runs a script, capturing out/err into $ROOT/out $ROOT/err and the exit code into RC.
run() { # script args...
  bash "$@" > "$ROOT/out" 2> "$ROOT/err"; RC=$?
}
calls() { cat "$LAUNCHCTL_LOG"; }

PLIST="$ROOT/agents/$LABEL.plist"

# --- install: plist content ---
reset; rm -rf "$ROOT/agents"
if need_script "install writes plist" "$INSTALL"; then
  run "$INSTALL" "$PLIST"
  check_eq "install exits 0" "$RC" "0"
  check_eq "install stdout" "$(cat "$ROOT/out")" "notch-hud: login item installed: $PLIST"
  if plutil -lint "$PLIST" >/dev/null 2>&1; then pass "plist passes plutil -lint"; else fail "plist passes plutil -lint" "lint failed or plist absent"; fi
  check_eq "Label" "$(extract "$PLIST" Label)" "$LABEL"
  check_eq "ProgramArguments.0" "$(extract "$PLIST" ProgramArguments.0)" "$BIN"
  check_eq "RunAtLoad" "$(extract "$PLIST" RunAtLoad)" "true"
  check_eq "KeepAlive.SuccessfulExit" "$(extract "$PLIST" KeepAlive.SuccessfulExit)" "false"
  check_eq "ProcessType" "$(extract "$PLIST" ProcessType)" "Interactive"
  check_eq "StandardErrorPath" "$(extract "$PLIST" StandardErrorPath)" "$HOME/Library/Logs/notch-hud.log"
  check_eq "plist mode" "$([ -f "$PLIST" ] && mode_of "$PLIST")" "644"
  check_eq "launchctl calls in order" "$(calls)" "bootout gui/$UID_/$LABEL
print gui/$UID_/$LABEL
bootstrap gui/$UID_ $PLIST"
  check_eq "no temp files beside plist" "$(ls -A "$ROOT/agents" 2>/dev/null)" "$LABEL.plist"
else
  for n in "install exits 0" "install stdout" "plist passes plutil -lint" "Label" "ProgramArguments.0" "RunAtLoad" \
    "KeepAlive.SuccessfulExit" "ProcessType" "StandardErrorPath" "plist mode" "launchctl calls in order" "no temp files beside plist"; do
    fail "$n" "script missing"
  done
fi

# --- install: launchctl failure modes ---
reset; rm -rf "$ROOT/agents"
if need_script "install tolerates failing bootout" "$INSTALL"; then
  FAKE_BOOTOUT_RC=3 run "$INSTALL" "$PLIST"
  check_eq "install succeeds when bootout fails" "$RC" "0"
  reset; rm -rf "$ROOT/agents"
  FAKE_BOOTSTRAP_RC=5 run "$INSTALL" "$PLIST"
  if [ "$RC" -ne 0 ]; then pass "install exits non-zero when bootstrap fails"; else fail "install exits non-zero when bootstrap fails" "exit 0"; fi
else
  fail "install succeeds when bootout fails" "script missing"
  fail "install exits non-zero when bootstrap fails" "script missing"
fi

# --- install: bad binaries ---
MSG="notch-hud: build first: ./scripts/build-app.sh"
for case_ in missing nonexec; do
  reset; rm -rf "$ROOT/agents"
  if [ "$case_" = missing ]; then bad="$ROOT/bin/nope"; else bad="$ROOT/bin/plain"; : > "$bad"; chmod -x "$bad"; fi
  name="$case_ binary"
  if need_script "$name" "$INSTALL"; then
    NOTCH_HUD_BIN="$bad" run "$INSTALL" "$PLIST"
    check_eq "$name: exit 1" "$RC" "1"
    check_eq "$name: stderr" "$(cat "$ROOT/err")" "$MSG"
    check_eq "$name: no plist" "$([ -e "$PLIST" ] && echo present || echo absent)" "absent"
    check_eq "$name: no launchctl calls" "$(calls)" ""
  else
    for n in "exit 1" "stderr" "no plist" "no launchctl calls"; do fail "$name: $n" "script missing"; done
  fi
done

# --- install: default binary is the app bundle's executable (NOTCH_HUD_BIN unset) ---
FAKE_REPO="$ROOT/repo"
FAKE_INSTALL="$FAKE_REPO/scripts/install-login.sh"
DEFAULT_BIN="$FAKE_REPO/src-tauri/target/notch-hud.app/Contents/MacOS/notch-hud"
mkdir -p "$FAKE_REPO/scripts"
if need_script "default binary" "$INSTALL"; then
  cp "$INSTALL" "$FAKE_INSTALL"
  reset; rm -rf "$ROOT/agents" "$FAKE_REPO/src-tauri"
  mkdir -p "$(dirname "$DEFAULT_BIN")"
  printf '#!/bin/sh\nexit 0\n' > "$DEFAULT_BIN"; chmod +x "$DEFAULT_BIN"
  unset NOTCH_HUD_BIN; run "$FAKE_INSTALL" "$PLIST"; export NOTCH_HUD_BIN="$BIN"
  check_eq "default bundle binary: exit 0" "$RC" "0"
  check_eq "default bundle binary: ProgramArguments.0" "$(extract "$PLIST" ProgramArguments.0)" "$DEFAULT_BIN"

  reset; rm -rf "$ROOT/agents" "$FAKE_REPO/src-tauri"
  unset NOTCH_HUD_BIN; run "$FAKE_INSTALL" "$PLIST"; export NOTCH_HUD_BIN="$BIN"
  check_eq "default bundle binary absent: exit 1" "$RC" "1"
  check_eq "default bundle binary absent: stderr" "$(cat "$ROOT/err")" "$MSG"
  check_eq "default bundle binary absent: no plist" "$([ -e "$PLIST" ] && echo present || echo absent)" "absent"
  check_eq "default bundle binary absent: no launchctl calls" "$(calls)" ""
else
  for n in "exit 0" "ProgramArguments.0"; do fail "default bundle binary: $n" "script missing"; done
  for n in "exit 1" "stderr" "no plist" "no launchctl calls"; do fail "default bundle binary absent: $n" "script missing"; done
fi

# --- install: binary path with & ---
reset; rm -rf "$ROOT/agents"
mkdir -p "$ROOT/a&b"; printf '#!/bin/sh\n' > "$ROOT/a&b/notch-hud"; chmod +x "$ROOT/a&b/notch-hud"
if need_script "ampersand binary" "$INSTALL"; then
  NOTCH_HUD_BIN="$ROOT/a&b/notch-hud" run "$INSTALL" "$PLIST"
  check_eq "ampersand binary: exit 1" "$RC" "1"
  check_eq "ampersand binary: stderr non-empty" "$([ -s "$ROOT/err" ] && echo yes || echo no)" "yes"
  check_eq "ampersand binary: no plist" "$([ -e "$PLIST" ] && echo present || echo absent)" "absent"
  check_eq "ampersand binary: no launchctl calls" "$(calls)" ""
else
  for n in "exit 1" "stderr non-empty" "no plist" "no launchctl calls"; do fail "ampersand binary: $n" "script missing"; done
fi

# --- install: missing directory, twice ---
reset; rm -rf "$ROOT/deep"
P2="$ROOT/deep/er/$LABEL.plist"
if need_script "install twice" "$INSTALL"; then
  run "$INSTALL" "$P2"; run "$INSTALL" "$P2"
  check_eq "creates missing plist directory" "$([ -f "$P2" ] && echo yes || echo no)" "yes"
  check_eq "twice leaves one plist, no temp files" "$(ls -A "$ROOT/deep/er")" "$LABEL.plist"
  check_eq "twice reloads both times" "$(calls)" "bootout gui/$UID_/$LABEL
print gui/$UID_/$LABEL
bootstrap gui/$UID_ $P2
bootout gui/$UID_/$LABEL
print gui/$UID_/$LABEL
bootstrap gui/$UID_ $P2"
else
  for n in "creates missing plist directory" "twice leaves one plist, no temp files" "twice reloads both times"; do fail "$n" "script missing"; done
fi

# --- install: old service lingers after bootout ---
reset; rm -rf "$ROOT/agents"
if need_script "lingering service" "$INSTALL"; then
  FAKE_LINGER=3 run "$INSTALL" "$PLIST"
  check_eq "lingers 3 prints: exit 0" "$RC" "0"
  check_eq "lingers 3 prints: bootstrap once, after the last print" "$(calls)" "bootout gui/$UID_/$LABEL
print gui/$UID_/$LABEL
print gui/$UID_/$LABEL
print gui/$UID_/$LABEL
print gui/$UID_/$LABEL
bootstrap gui/$UID_ $PLIST"

  reset; rm -rf "$ROOT/agents"
  t0="$(date +%s)"
  FAKE_LINGER=1000000 run "$INSTALL" "$PLIST"
  elapsed=$(( $(date +%s) - t0 ))
  check_eq "never stops: exit 1" "$RC" "1"
  check_eq "never stops: stderr" "$(cat "$ROOT/err")" "notch-hud: old login item did not stop"
  check_eq "never stops: no bootstrap" "$(calls | grep -c '^bootstrap')" "0"
  if [ "$elapsed" -le 6 ]; then pass "never stops: gives up within 6 s"; else fail "never stops: gives up within 6 s" "took ${elapsed}s"; fi
else
  for n in "lingers 3 prints: exit 0" "lingers 3 prints: bootstrap once, after the last print" "never stops: exit 1" \
    "never stops: stderr" "never stops: no bootstrap" "never stops: gives up within 6 s"; do fail "$n" "script missing"; done
fi

# --- uninstall ---
reset; mkdir -p "$ROOT/u"; P3="$ROOT/u/$LABEL.plist"; echo x > "$P3"
if need_script "uninstall" "$UNINSTALL"; then
  run "$UNINSTALL" "$P3"
  check_eq "uninstall exits 0" "$RC" "0"
  check_eq "uninstall calls bootout" "$(calls)" "bootout gui/$UID_/$LABEL"
  check_eq "uninstall removes plist" "$([ -e "$P3" ] && echo present || echo absent)" "absent"
  reset
  FAKE_BOOTOUT_RC=3 run "$UNINSTALL" "$P3"
  check_eq "uninstall exits 0 when nothing installed" "$RC" "0"
else
  for n in "uninstall exits 0" "uninstall calls bootout" "uninstall removes plist" "uninstall exits 0 when nothing installed"; do fail "$n" "script missing"; done
fi

echo
if [ "$FAILS" -eq 0 ]; then echo "all passed"; else echo "$FAILS failed"; fi
[ "$FAILS" -eq 0 ]
