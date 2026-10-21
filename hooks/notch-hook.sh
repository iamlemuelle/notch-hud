#!/bin/sh
# Records a Claude Code session's latest event for the notch app. Silent and exit 0 on every path.
exec >/dev/null 2>&1
dir="${NOTCH_DIR:-$HOME/.claude/notch/sessions}"
input=$(cat) || exit 0
# One jq call: prints "<action>\t<session_id>\t<record json>", or nothing when the event is skipped.
out=$(printf '%s' "$input" | /usr/bin/jq -r --argjson ts "$(date +%s)" '
  (.session_id // "") as $id
  | (.hook_event_name // "") as $ev
  | (.notification_type // null) as $nt
  | if ($id | type) != "string" or ($id | test("^[A-Za-z0-9_-]+$") | not) then empty
    elif $ev == "SessionEnd" then "delete\t\($id)\t"
    elif (has("agent_id") and .agent_id != null) and ($ev != "PermissionRequest" and $ev != "Notification") then empty
    elif $ev == "Notification" and ($nt as $t | ["permission_prompt","idle_prompt","elicitation_dialog","agent_needs_input"] | index($t) | not) then empty
    else "write\t\($id)\t" + ({session_id: $id, cwd: (.cwd // ""), event: $ev, notification_type: $nt,
         tool_name: (.tool_name // null), ts: $ts} | tojson)
    end') || exit 0
[ -n "$out" ] || exit 0
tab=$(printf '\t')
action=${out%%"$tab"*}
rest=${out#*"$tab"}
id=${rest%%"$tab"*}
record=${rest#*"$tab"}
case "$action" in
  delete) rm -f "$dir/$id.json" ;;
  write)
    mkdir -p "$dir" || exit 0
    tmp=$(mktemp "$dir/.$id.XXXXXX") || exit 0
    if printf '%s\n' "$record" >"$tmp" && mv -f "$tmp" "$dir/$id.json"; then :; else rm -f "$tmp"; fi
    ;;
esac
exit 0
