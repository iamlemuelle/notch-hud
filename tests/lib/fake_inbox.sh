# Sourced by tests/inbox_test.sh and tests/prompt_test.sh: the fake Claude Code inbox, kept in one place because it
# is the part that must never reach a real session. The caller has already run
# `unset CLAUDE_CODE_MESSAGING_SOCKET CLAUDE_CODE_MESSAGING_TOKEN`, and the socket lives in its own temp dir.
# Needs $root, $tmp, $sock, $received, $inboxlog and a wait_for function (log, seconds, fixed string).

# A sockaddr_un path holds 103 bytes and a NUL.
fake_inbox_check_path() {
  if [ "${#sock}" -ge 104 ]; then echo "SKIP the fake inbox path is ${#sock} bytes, the limit is 103: $sock"; exit 2; fi
}

# Waits up to $1 seconds for the fake's file to hold exactly the bytes of file $2.
wait_received() {
  local i=0
  while [ "$i" -lt $(($1 * 5)) ]; do
    cmp -s "$received" "$2" && return 0
    sleep 0.2; i=$((i + 1))
  done
  return 1
}

# Accepts connections one at a time and appends every byte it gets to $received. Sets inbox_pid.
start_fake_inbox() {
  python3 -I "$root/tests/lib/fake_inbox.py" "$sock" "$received" >"$inboxlog" 2>&1 & inbox_pid=$!
  wait_for "$inboxlog" 5 listening || { echo "FAIL the fake inbox did not start"; cat "$inboxlog"; exit 2; }
}
