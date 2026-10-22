#!/bin/sh
# Records a Claude Code session's latest event for the notch app. Silent and exit 0 on every path.
exec >/dev/null 2>&1
dir="${NOTCH_DIR:-$HOME/.claude/notch/sessions}"
input=$(cat) || exit 0
# One jq call: prints "<action>\t<session_id>\t<agent_id>\t<record json>", or nothing when the event is skipped.
# \A...\z anchor the whole string: "$" would accept a trailing newline.
# inbox and host come from the environment through --arg, never through the shell.
out=$(printf '%s' "$input" | /usr/bin/jq -r --argjson ts "$(date +%s)" \
  --arg inbox "${CLAUDE_CODE_MESSAGING_SOCKET:-}" --arg host "${__CFBundleIdentifier:-}" '
  def ok: type == "string" and test("\\A[A-Za-z0-9_-]+\\z");
  def nonempty: if . == "" then null else . end;
  (.session_id // "") as $id
  | (.agent_id // null) as $ag
  | (.hook_event_name // "") as $ev
  | (.notification_type // null) as $nt
  | if ($id | ok | not) or ($ag != null and ($ag | ok | not)) then empty
    elif $ev == "SessionEnd" then "delete_session\t\($id)\t\t"
    elif $ev == "SubagentStop" then (if $ag == null then empty else "delete\t\($id)\t\($ag)\t" end)
    elif $ev == "Notification" and ($nt as $t | ["permission_prompt","idle_prompt","elicitation_dialog","agent_needs_input"] | index($t) | not) then empty
    else "write\t\($id)\t\($ag // "")\t" + ({session_id: $id, agent_id: $ag, cwd: (.cwd // ""), event: $ev, notification_type: $nt,
         tool_name: (.tool_name // null), ts: $ts, inbox: ($inbox | nonempty), host: ($host | nonempty)} | tojson)
    end') || exit 0
[ -n "$out" ] || exit 0
tab=$(printf '\t')
action=${out%%"$tab"*}
rest=${out#*"$tab"}
id=${rest%%"$tab"*}
rest=${rest#*"$tab"}
agent=${rest%%"$tab"*}
record=${rest#*"$tab"}
name="$id"
[ -z "$agent" ] || name="$id.$agent"
case "$action" in
  delete_session) rm -f "$dir/$id.json" "$dir/$id".*.json ;;
  delete) rm -f "$dir/$name.json" ;;
  write)
    mkdir -p "$dir" || exit 0
    tmp=$(mktemp "$dir/.$name.XXXXXX") || exit 0
    if printf '%s\n' "$record" >"$tmp" && mv -f "$tmp" "$dir/$name.json"; then :; else rm -f "$tmp"; fi
    ;;
esac
exit 0
