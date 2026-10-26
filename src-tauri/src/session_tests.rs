use super::*;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, UNIX_EPOCH};

const NOW: u64 = 1_800_000_000;

fn rec(id: &str, cwd: &str, event: &str, ts: u64) -> Record {
    Record {
        session_id: id.to_string(),
        cwd: cwd.to_string(),
        event: event.to_string(),
        agent_id: None,
        notification_type: None,
        tool_name: None,
        ts,
        inbox: None,
        host: None,
    }
}

fn sub(id: &str, agent: &str, cwd: &str, event: &str, ts: u64) -> Record {
    Record { agent_id: Some(agent.to_string()), ..rec(id, cwd, event, ts) }
}

fn set_mtime(path: &Path, secs: u64) {
    let f = fs::OpenOptions::new().write(true).open(path).unwrap();
    f.set_modified(UNIX_EPOCH + Duration::from_secs(secs)).unwrap();
}

fn write_json(dir: &Path, file: &str, id: &str, mtime: u64) {
    let p = dir.join(file);
    fs::write(&p, json_for(id)).unwrap();
    set_mtime(&p, mtime);
}

fn mtime_secs(path: &Path) -> u64 {
    fs::metadata(path).unwrap().modified().unwrap().duration_since(UNIX_EPOCH).unwrap().as_secs()
}

fn notif(nt: &str) -> Record {
    Record { notification_type: Some(nt.to_string()), ..rec("s", "/a", "Notification", NOW) }
}

fn tmp(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("notch-session-tests-{}-{}", std::process::id(), name));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    d
}

fn json_for(id: &str) -> String {
    format!(
        r#"{{"session_id":"{id}","cwd":"/x/{id}","event":"Stop","notification_type":null,"tool_name":null,"ts":100}}"#
    )
}

#[test]
fn status_of_event_table() {
    let cases = [
        ("SessionStart", Some(Status::Waiting)),
        ("UserPromptSubmit", Some(Status::Working)),
        ("PreToolUse", Some(Status::Working)),
        ("PostToolUse", Some(Status::Working)),
        ("PostToolUseFailure", Some(Status::Working)),
        ("PermissionRequest", Some(Status::NeedsPermission)),
        ("PermissionDenied", Some(Status::Waiting)),
        ("Stop", Some(Status::Done)),
        ("StopFailure", Some(Status::Error)),
        ("SessionEnd", None),
        ("SubagentStop", None),
        ("Bogus", None),
        ("", None),
    ];
    for (event, want) in cases {
        assert_eq!(status_of(&rec("s", "/a", event, NOW)), want, "event {event}");
    }
}

#[test]
fn status_of_notification_types() {
    assert_eq!(status_of(&notif("permission_prompt")), Some(Status::NeedsPermission));
    assert_eq!(status_of(&notif("idle_prompt")), Some(Status::Waiting));
    assert_eq!(status_of(&notif("elicitation_dialog")), Some(Status::NeedsInput));
    assert_eq!(status_of(&notif("agent_needs_input")), Some(Status::NeedsInput));
    assert_eq!(status_of(&notif("auth_success")), None);
    assert_eq!(status_of(&rec("s", "/a", "Notification", NOW)), None);
}

#[test]
fn summarize_empty() {
    assert_eq!(
        summarize(&[], NOW),
        Summary { rows: vec![], total: 0, attention: 0, working: 0 }
    );
}

#[test]
fn summarize_ttl_boundary() {
    let recs = [
        rec("keep", "/p/keep", "Stop", NOW - TTL_SECS),
        rec("drop", "/p/drop", "Stop", NOW - TTL_SECS - 1),
    ];
    let s = summarize(&recs, NOW);
    assert_eq!(s.total, 1);
    assert_eq!(
        s.rows,
        vec![Row { id: "keep".into(), label: "keep".into(), status: Status::Done, detail: None }]
    );
}

#[test]
fn summarize_future_ts_kept() {
    let s = summarize(&[rec("f", "/p/fut", "PreToolUse", NOW + 500)], NOW);
    assert_eq!(s.total, 1);
    assert_eq!(s.working, 1);
    assert_eq!(s.rows[0].label, "fut");
}

#[test]
fn summarize_ignored_events_not_counted() {
    let recs = [rec("a", "/p/a", "SessionEnd", NOW), rec("b", "/p/b", "Bogus", NOW)];
    let s = summarize(&recs, NOW);
    assert_eq!(s.total, 0);
    assert!(s.rows.is_empty());
}

#[test]
fn summarize_labels_from_cwd() {
    let cases = [
        ("/Users/lem/bianki", "bianki"),
        ("/Users/lem/bianki/", "bianki"),
        ("/Users/lem/bianki//", "bianki"),
        ("/", "/"),
        ("solo", "solo"),
    ];
    for (cwd, want) in cases {
        let s = summarize(&[rec("abcd1234", cwd, "Stop", NOW)], NOW);
        assert_eq!(s.rows[0].label, want, "cwd {cwd:?}");
    }
}

#[test]
fn summarize_duplicate_labels_suffixed() {
    let recs = [
        rec("aaaa1111", "/one/app", "Stop", NOW),
        rec("bbbb2222", "/two/app", "Stop", NOW - 1),
        rec("cccc3333", "/three/other", "Stop", NOW - 2),
    ];
    let s = summarize(&recs, NOW);
    let labels: Vec<&str> = s.rows.iter().map(|r| r.label.as_str()).collect();
    assert_eq!(labels, vec!["app · aaaa", "app · bbbb", "other"]);
}

#[test]
fn summarize_duplicate_only_counts_kept() {
    let recs = [
        rec("aaaa1111", "/one/app", "Stop", NOW),
        rec("bbbb2222", "/two/app", "Stop", NOW - TTL_SECS - 1),
    ];
    assert_eq!(summarize(&recs, NOW).rows[0].label, "app");
}

#[test]
fn summarize_detail_only_for_needs_permission() {
    let mut perm = rec("p", "/p/perm", "PermissionRequest", NOW);
    perm.tool_name = Some("Bash".into());
    let mut notif_perm = rec("q", "/p/np", "Notification", NOW);
    notif_perm.notification_type = Some("permission_prompt".into());
    notif_perm.tool_name = Some("Edit".into());
    let mut working = rec("w", "/p/work", "PreToolUse", NOW);
    working.tool_name = Some("Read".into());
    let mut input = rec("i", "/p/in", "Notification", NOW);
    input.notification_type = Some("elicitation_dialog".into());
    input.tool_name = Some("Ask".into());
    let no_tool = rec("n", "/p/notool", "PermissionRequest", NOW - 1);

    let s = summarize(&[perm, notif_perm, working, input, no_tool], NOW);
    let got: Vec<(String, Option<String>)> =
        s.rows.iter().map(|r| (r.label.clone(), r.detail.clone())).collect();
    assert_eq!(
        got,
        vec![
            ("perm".to_string(), Some("Bash".to_string())),
            ("np".to_string(), Some("Edit".to_string())),
            ("notool".to_string(), None),
            ("in".to_string(), None),
        ]
    );
}

#[test]
fn summarize_sort_priority_order() {
    let mut input = rec("5", "/p/input", "Notification", NOW);
    input.notification_type = Some("agent_needs_input".into());
    let recs = [
        rec("1", "/p/waiting", "SessionStart", NOW),
        rec("2", "/p/done", "Stop", NOW),
        rec("3", "/p/working", "PreToolUse", NOW),
        rec("4", "/p/error", "StopFailure", NOW),
        input,
        rec("6", "/p/perm", "PermissionRequest", NOW),
    ];
    let s = summarize(&recs, NOW);
    let statuses: Vec<Status> = s.rows.iter().map(|r| r.status).collect();
    assert_eq!(
        statuses,
        vec![
            Status::NeedsPermission,
            Status::NeedsInput,
            Status::Error,
            Status::Working
        ]
    );
}

#[test]
fn summarize_sort_priority_order_below_working() {
    let recs = [
        rec("1", "/p/waiting", "SessionStart", NOW),
        rec("2", "/p/done", "Stop", NOW),
        rec("3", "/p/working", "PreToolUse", NOW),
    ];
    let s = summarize(&recs, NOW);
    let statuses: Vec<Status> = s.rows.iter().map(|r| r.status).collect();
    assert_eq!(statuses, vec![Status::Working, Status::Done, Status::Waiting]);
}

#[test]
fn summarize_sort_ts_desc_then_session_id_asc() {
    let recs = [
        rec("b", "/p/b", "Stop", 100),
        rec("a", "/p/a", "Stop", 100),
        rec("c", "/p/c", "Stop", 200),
        rec("d", "/p/d", "Stop", 50),
    ];
    let s = summarize(&recs, 300);
    let labels: Vec<&str> = s.rows.iter().map(|r| r.label.as_str()).collect();
    assert_eq!(labels, vec!["c", "a", "b", "d"]);
}

#[test]
fn summarize_truncates_but_counts_everything() {
    let mut recs = Vec::new();
    for i in 0..4u64 {
        recs.push(rec(&format!("w{i}"), &format!("/p/w{i}"), "PreToolUse", NOW - i));
    }
    for i in 0..3u64 {
        recs.push(rec(&format!("p{i}"), &format!("/p/p{i}"), "PermissionRequest", NOW - i));
    }
    let mut ni = rec("n0", "/p/n0", "Notification", NOW);
    ni.notification_type = Some("elicitation_dialog".into());
    recs.push(ni);
    recs.push(rec("d0", "/p/d0", "Stop", NOW));
    recs.push(rec("old", "/p/old", "PreToolUse", NOW - TTL_SECS - 1));

    let s = summarize(&recs, NOW);
    assert_eq!(s.total, 9);
    assert_eq!(s.attention, 4);
    assert_eq!(s.working, 4);
    assert_eq!(s.rows.len(), MAX_ROWS);
    let labels: Vec<&str> = s.rows.iter().map(|r| r.label.as_str()).collect();
    assert_eq!(labels, vec!["p0", "p1", "p2", "n0"]);
}

#[test]
fn summarize_working_subagent_cannot_hide_needs_permission() {
    let mut perm = sub("s", "ag1", "/p/proj", "PermissionRequest", NOW - 20);
    perm.tool_name = Some("Bash".into());
    let recs = [rec("s", "/p/proj", "PreToolUse", NOW - 5), perm];
    let s = summarize(&recs, NOW);
    assert_eq!(
        s,
        Summary {
            rows: vec![Row { id: "s".into(), label: "proj".into(), status: Status::NeedsPermission, detail: Some("Bash".into()) }],
            total: 1,
            attention: 1,
            working: 0,
        }
    );
}

#[test]
fn summarize_main_permission_beats_newer_subagent_working() {
    let mut perm = rec("s", "/p/proj", "PermissionRequest", NOW - 20);
    perm.tool_name = Some("Edit".into());
    let recs = [perm, sub("s", "ag1", "/p/proj", "PreToolUse", NOW - 1)];
    let s = summarize(&recs, NOW);
    assert_eq!(
        s.rows,
        vec![Row { id: "s".into(), label: "proj".into(), status: Status::NeedsPermission, detail: Some("Edit".into()) }]
    );
    assert_eq!((s.total, s.attention, s.working), (1, 1, 0));
}

#[test]
fn summarize_main_done_with_working_subagent_is_working() {
    let recs = [
        rec("s", "/p/proj", "Stop", NOW - 30),
        sub("s", "ag1", "/p/proj", "PreToolUse", NOW - 2),
    ];
    let s = summarize(&recs, NOW);
    assert_eq!(s.rows, vec![Row { id: "s".into(), label: "proj".into(), status: Status::Working, detail: None }]);
    assert_eq!((s.total, s.attention, s.working), (1, 0, 1));
}

#[test]
fn summarize_counts_sessions_not_records() {
    let mut elicit = sub("s", "ag2", "/p/proj", "Notification", NOW);
    elicit.notification_type = Some("elicitation_dialog".into());
    let recs = [
        rec("s", "/p/proj", "PermissionRequest", NOW),
        sub("s", "ag1", "/p/proj", "PermissionRequest", NOW),
        elicit,
        rec("t", "/p/other", "Stop", NOW),
        sub("t", "ag1", "/p/other", "Stop", NOW),
    ];
    let s = summarize(&recs, NOW);
    assert_eq!((s.total, s.attention, s.working), (2, 1, 0));
    assert_eq!(s.rows.len(), 2);
}

#[test]
fn summarize_session_ts_is_max_over_group() {
    // Session "a" has an old main record and a newer subagent record, so it sorts before "b".
    let recs = [
        rec("a", "/p/a", "Stop", 100),
        sub("a", "ag1", "/p/a", "Stop", 300),
        rec("b", "/p/b", "Stop", 200),
    ];
    let labels: Vec<String> = summarize(&recs, 400).rows.into_iter().map(|r| r.label).collect();
    assert_eq!(labels, vec!["a", "b"]);
}

#[test]
fn summarize_quiet_decay_boundary() {
    let at = summarize(&[rec("s", "/p/proj", "PreToolUse", NOW - QUIET_SECS)], NOW);
    assert_eq!(at.rows[0].status, Status::Working);
    assert_eq!(at.working, 1);

    let past = summarize(&[rec("s", "/p/proj", "PreToolUse", NOW - QUIET_SECS - 1)], NOW);
    assert_eq!(past.rows[0].status, Status::Waiting);
    assert_eq!(past.working, 0);
    assert_eq!(past.total, 1);
    assert_eq!(past.attention, 0);
}

#[test]
fn summarize_decay_uses_session_ts_not_record_ts() {
    // The Working main record is stale but a Done subagent record is fresh: Working wins, ts is the fresh one.
    let recs = [
        rec("s", "/p/proj", "PreToolUse", NOW - QUIET_SECS - 500),
        sub("s", "ag1", "/p/proj", "Stop", NOW - 3),
    ];
    let s = summarize(&recs, NOW);
    assert_eq!(s.rows[0].status, Status::Working);
    assert_eq!(s.working, 1);
}

#[test]
fn summarize_decay_applies_only_to_working() {
    let recs = [
        rec("d", "/p/done", "Stop", NOW - QUIET_SECS - 1),
        rec("p", "/p/perm", "PermissionRequest", NOW - QUIET_SECS - 1),
    ];
    let statuses: Vec<Status> = summarize(&recs, NOW).rows.iter().map(|r| r.status).collect();
    assert_eq!(statuses, vec![Status::NeedsPermission, Status::Done]);
}

#[test]
fn summarize_cwd_from_main_record() {
    let recs = [
        sub("s", "ag1", "/p/worktree-sub", "PermissionRequest", NOW),
        rec("s", "/p/mainproj", "PreToolUse", NOW - 1),
    ];
    let s = summarize(&recs, NOW);
    assert_eq!(s.rows[0].label, "mainproj");
    assert_eq!(s.rows[0].status, Status::NeedsPermission);
}

#[test]
fn summarize_cwd_from_status_record_without_main() {
    let recs = [
        sub("s", "ag1", "/p/quiet", "Stop", NOW),
        sub("s", "ag2", "/p/loud", "PermissionRequest", NOW - 10),
    ];
    let s = summarize(&recs, NOW);
    assert_eq!(s.rows[0].label, "loud");
    assert_eq!(s.rows[0].status, Status::NeedsPermission);
}

#[test]
fn summarize_permission_denied_is_waiting() {
    let s = summarize(&[rec("s", "/p/proj", "PermissionDenied", NOW)], NOW);
    assert_eq!(s.rows, vec![Row { id: "s".into(), label: "proj".into(), status: Status::Waiting, detail: None }]);
    assert_eq!((s.total, s.attention, s.working), (1, 0, 0));
}

#[test]
fn summarize_duplicate_labels_across_grouped_sessions() {
    let recs = [
        rec("aaaa1111", "/one/app", "Stop", NOW),
        sub("aaaa1111", "ag1", "/one/app", "Stop", NOW),
        rec("bbbb2222", "/two/app", "Stop", NOW - 1),
    ];
    let labels: Vec<String> = summarize(&recs, NOW).rows.into_iter().map(|r| r.label).collect();
    assert_eq!(labels, vec!["app · aaaa", "app · bbbb"]);
}

#[test]
fn read_dir_missing_returns_empty() {
    let d = std::env::temp_dir().join(format!("notch-session-tests-{}-missing", std::process::id()));
    let _ = fs::remove_dir_all(&d);
    assert_eq!(read_dir(&d), Vec::<Record>::new());
}

#[test]
fn read_dir_parses_skips_and_ties_by_name() {
    let d = tmp("readdir");
    for f in ["b.json", "a.json", "bad.json", "notes.txt", "c.json.tmp"] {
        let id = f.split('.').next().unwrap();
        let body = if f == "bad.json" { "{not json".to_string() } else { json_for(id) };
        fs::write(d.join(f), body).unwrap();
        set_mtime(&d.join(f), NOW);
    }
    fs::create_dir(d.join("dir.json")).unwrap();
    let got = read_dir(&d);
    let want = |id: &str| Record {
        session_id: id.into(),
        cwd: format!("/x/{id}"),
        event: "Stop".into(),
        agent_id: None,
        notification_type: None,
        tool_name: None,
        ts: 100,
        inbox: None,
        host: None,
    };
    assert_eq!(got, vec![want("a"), want("b")]);
    let _ = fs::remove_dir_all(&d);
}

#[test]
fn read_dir_orders_by_mtime_descending() {
    let d = tmp("mtime-order");
    write_json(&d, "a.json", "a", NOW - 30);
    write_json(&d, "b.json", "b", NOW - 10);
    write_json(&d, "c.json", "c", NOW - 20);
    let ids: Vec<String> = read_dir(&d).into_iter().map(|r| r.session_id).collect();
    assert_eq!(ids, vec!["b", "c", "a"]);
    let _ = fs::remove_dir_all(&d);
}

#[test]
fn read_dir_parses_agent_id() {
    let d = tmp("agent-id");
    fs::write(
        d.join("s.ag1.json"),
        r#"{"session_id":"s","agent_id":"ag1","cwd":"/c","event":"PreToolUse","notification_type":null,"tool_name":"Read","ts":9}"#,
    )
    .unwrap();
    let got = read_dir(&d);
    assert_eq!(got.len(), 1);
    assert_eq!(got[0].agent_id.as_deref(), Some("ag1"));
    assert_eq!(got[0].session_id, "s");
    let _ = fs::remove_dir_all(&d);
}

#[test]
fn read_dir_optional_fields_default_and_present() {
    let d = tmp("optional");
    fs::write(
        d.join("x.json"),
        r#"{"session_id":"x","cwd":"/c","event":"PermissionRequest","tool_name":"Bash","ts":7}"#,
    )
    .unwrap();
    fs::write(
        d.join("y.json"),
        r#"{"session_id":"y","cwd":"/c","event":"Notification","notification_type":"idle_prompt","tool_name":null,"ts":8}"#,
    )
    .unwrap();
    let got = read_dir(&d);
    assert_eq!(got.len(), 2);
    // read_dir orders by mtime (contract section 2), so look records up by id rather than position.
    let x = got.iter().find(|r| r.session_id == "x").expect("x read");
    let y = got.iter().find(|r| r.session_id == "y").expect("y read");
    assert_eq!(x.tool_name.as_deref(), Some("Bash"));
    assert_eq!(x.notification_type, None);
    assert_eq!(y.notification_type.as_deref(), Some("idle_prompt"));
    assert_eq!(y.tool_name, None);
    let _ = fs::remove_dir_all(&d);
}

#[test]
fn read_dir_parses_inbox_and_host_with_and_without_them() {
    let d = tmp("inbox-host");
    fs::write(
        d.join("a.json"),
        r#"{"session_id":"a","agent_id":null,"cwd":"/c","event":"Stop","notification_type":null,"tool_name":null,"ts":5,"inbox":"/tmp/cc-socks/41.sock","host":"com.anthropic.claudefordesktop"}"#,
    )
    .unwrap();
    fs::write(
        d.join("b.json"),
        r#"{"session_id":"b","agent_id":null,"cwd":"/c","event":"Stop","notification_type":null,"tool_name":null,"ts":5,"inbox":null,"host":null}"#,
    )
    .unwrap();
    fs::write(d.join("c.json"), r#"{"session_id":"c","cwd":"/c","event":"Stop","ts":5}"#).unwrap();
    let mut got = read_dir(&d);
    got.sort_by(|x, y| x.session_id.cmp(&y.session_id));
    let a = Record {
        inbox: Some("/tmp/cc-socks/41.sock".into()),
        host: Some("com.anthropic.claudefordesktop".into()),
        ..rec("a", "/c", "Stop", 5)
    };
    assert_eq!(got, vec![a, rec("b", "/c", "Stop", 5), rec("c", "/c", "Stop", 5)]);
    let _ = fs::remove_dir_all(&d);
}

#[test]
fn read_dir_keeps_newest_max_files_by_mtime() {
    let d = tmp("maxfiles-mtime");
    // Name order is the reverse of age order, so a name-based cap would keep the wrong files.
    for i in 0..(MAX_FILES + 10) {
        let id = format!("s{i:03}");
        write_json(&d, &format!("{id}.json"), &id, NOW - 1000 + i as u64);
    }
    let mut ids: Vec<String> = read_dir(&d).into_iter().map(|r| r.session_id).collect();
    assert_eq!(ids.len(), MAX_FILES);
    ids.sort();
    let want: Vec<String> = (10..(MAX_FILES + 10)).map(|i| format!("s{i:03}")).collect();
    assert_eq!(ids, want);
    let _ = fs::remove_dir_all(&d);
}

#[test]
fn read_dir_max_files_ties_break_by_name() {
    let d = tmp("maxfiles-tie");
    for i in 0..(MAX_FILES + 1) {
        let id = format!("t{i:03}");
        write_json(&d, &format!("{id}.json"), &id, NOW - 5);
    }
    let mut ids: Vec<String> = read_dir(&d).into_iter().map(|r| r.session_id).collect();
    ids.sort();
    let want: Vec<String> = (0..MAX_FILES).map(|i| format!("t{i:03}")).collect();
    assert_eq!(ids, want);
    let _ = fs::remove_dir_all(&d);
}

#[test]
fn prune_deletes_only_expired_json() {
    let d = tmp("prune");
    write_json(&d, "old.json", "old", NOW - TTL_SECS - 1);
    write_json(&d, "edge.json", "edge", NOW - TTL_SECS);
    write_json(&d, "fresh.json", "fresh", NOW - 5);
    write_json(&d, "old.sub1.json", "old", NOW - TTL_SECS - 100);
    for other in ["old.txt", "old.json.tmp", "old.json.bak"] {
        fs::write(d.join(other), "x").unwrap();
        set_mtime(&d.join(other), NOW - TTL_SECS - 1000);
    }
    prune(&d, NOW);
    let mut left: Vec<String> = fs::read_dir(&d)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    left.sort();
    assert_eq!(left, vec!["edge.json", "fresh.json", "old.json.bak", "old.json.tmp", "old.txt"]);
    assert_eq!(mtime_secs(&d.join("fresh.json")), NOW - 5);
    let _ = fs::remove_dir_all(&d);
}

#[test]
fn prune_missing_dir_does_not_panic() {
    let d = std::env::temp_dir().join(format!("notch-session-tests-{}-prune-missing", std::process::id()));
    let _ = fs::remove_dir_all(&d);
    prune(&d, NOW);
    assert!(!d.exists());
}

#[test]
fn sessions_dir_env_handling() {
    let old_dir = std::env::var_os("NOTCH_DIR");
    let old_home = std::env::var_os("HOME");

    std::env::set_var("NOTCH_DIR", "/tmp/custom-notch-dir");
    assert_eq!(sessions_dir(), PathBuf::from("/tmp/custom-notch-dir"));

    std::env::set_var("HOME", "/home/someone");
    std::env::set_var("NOTCH_DIR", "");
    assert_eq!(sessions_dir(), PathBuf::from("/home/someone/.claude/notch/sessions"));

    std::env::remove_var("NOTCH_DIR");
    assert_eq!(sessions_dir(), PathBuf::from("/home/someone/.claude/notch/sessions"));

    match old_dir {
        Some(v) => std::env::set_var("NOTCH_DIR", v),
        None => std::env::remove_var("NOTCH_DIR"),
    }
    match old_home {
        Some(v) => std::env::set_var("HOME", v),
        None => std::env::remove_var("HOME"),
    }
}

#[test]
fn contract_read_dir_caps_at_50_files() {
    let d = tmp("contract-50");
    for i in 0..51u64 {
        let id = format!("c{i:02}");
        write_json(&d, &format!("{id}.json"), &id, NOW - 1000 + i);
    }
    let mut ids: Vec<String> = read_dir(&d).into_iter().map(|r| r.session_id).collect();
    assert_eq!(ids.len(), 50);
    ids.sort();
    assert!(!ids.contains(&"c00".to_string()));
    assert!(ids.contains(&"c01".to_string()));
    assert!(ids.contains(&"c50".to_string()));
    let _ = fs::remove_dir_all(&d);
}

#[test]
fn contract_summarize_caps_at_4_rows() {
    let recs: Vec<Record> = (0..7u64)
        .map(|i| rec(&format!("r{i}"), &format!("/p/r{i}"), "Stop", NOW - i))
        .collect();
    let s = summarize(&recs, NOW);
    assert_eq!(s.total, 7);
    assert_eq!(s.rows.len(), 4);
}

#[test]
fn summarize_seven_sessions_show_four_rows_and_total_seven() {
    let recs: Vec<Record> = (0..7u64)
        .map(|i| rec(&format!("s{i}"), &format!("/p/s{i}"), "PreToolUse", NOW - i))
        .collect();
    let s = summarize(&recs, NOW);
    assert_eq!(s.rows.len(), 4);
    assert_eq!(s.total, 7);
    assert_eq!(s.working, 7);
}

#[test]
fn summarize_lone_error_session_counts_as_attention() {
    let s = summarize(&[rec("e", "/p/boom", "StopFailure", NOW)], NOW);
    assert_eq!(s.rows.len(), 1);
    assert_eq!(s.rows[0].status, Status::Error);
    assert_eq!((s.total, s.attention, s.working), (1, 1, 0));
}

#[test]
fn contract_ttl_is_10800_seconds() {
    let recs = [
        rec("keep", "/p/keep", "Stop", NOW - 10800),
        rec("drop", "/p/drop", "Stop", NOW - 10801),
    ];
    let s = summarize(&recs, NOW);
    assert_eq!(s.total, 1);
    assert_eq!(s.rows[0].label, "keep");

    let d = tmp("contract-ttl");
    write_json(&d, "keep.json", "keep", NOW - 10800);
    write_json(&d, "drop.json", "drop", NOW - 10801);
    prune(&d, NOW);
    assert!(d.join("keep.json").exists());
    assert!(!d.join("drop.json").exists());
    let _ = fs::remove_dir_all(&d);
}

#[test]
fn contract_quiet_decay_is_600_seconds() {
    let at = summarize(&[rec("s", "/p/proj", "PreToolUse", NOW - 600)], NOW);
    assert_eq!(at.rows[0].status, Status::Working);
    let past = summarize(&[rec("s", "/p/proj", "PreToolUse", NOW - 601)], NOW);
    assert_eq!(past.rows[0].status, Status::Waiting);
}

const STALE: u64 = NOW - 100;
const BOUNDARY: u64 = NOW - 50;
const NEWER: u64 = NOW - 10;

fn stale_perm() -> Record {
    let mut p = sub("s", "ag1", "/p/proj", "PermissionRequest", STALE);
    p.tool_name = Some("Bash".into());
    p
}

fn one_row(recs: &[Record]) -> (Row, usize) {
    let s = summarize(recs, NOW);
    assert_eq!(s.total, 1);
    (s.rows[0].clone(), s.attention)
}

#[test]
fn supersede_boundaries_clear_stale_agent_permission() {
    let idle = Record { notification_type: Some("idle_prompt".into()), ..rec("s", "/p/proj", "Notification", BOUNDARY) };
    let cases = [
        (rec("s", "/p/proj", "UserPromptSubmit", BOUNDARY), Status::Working),
        (rec("s", "/p/proj", "Stop", BOUNDARY), Status::Done),
        (rec("s", "/p/proj", "StopFailure", BOUNDARY), Status::Error),
        (rec("s", "/p/proj", "SessionStart", BOUNDARY), Status::Waiting),
        (rec("s", "/p/proj", "PermissionDenied", BOUNDARY), Status::Waiting),
        (idle, Status::Waiting),
    ];
    for (main, want) in cases {
        let label = format!("{} {:?}", main.event, main.notification_type);
        let (row, attention) = one_row(&[stale_perm(), main]);
        assert_eq!(row, Row { id: "s".into(), label: "proj".into(), status: want, detail: None }, "{label}");
        assert_eq!(attention, usize::from(want == Status::Error), "{label}");
    }
}

#[test]
fn supersede_main_non_boundary_keeps_agent_permission() {
    for event in ["PreToolUse", "PostToolUse", "PostToolUseFailure"] {
        let (row, attention) = one_row(&[stale_perm(), rec("s", "/p/proj", event, BOUNDARY)]);
        assert_eq!(
            row,
            Row { id: "s".into(), label: "proj".into(), status: Status::NeedsPermission, detail: Some("Bash".into()) },
            "{event}"
        );
        assert_eq!(attention, 1, "{event}");
    }
}

#[test]
fn supersede_main_permission_prompt_notification_is_not_boundary() {
    let main = Record { notification_type: Some("permission_prompt".into()), ..rec("s", "/p/proj", "Notification", BOUNDARY) };
    let (row, attention) = one_row(&[stale_perm(), main]);
    assert_eq!(
        row,
        Row { id: "s".into(), label: "proj".into(), status: Status::NeedsPermission, detail: Some("Bash".into()) }
    );
    assert_eq!(attention, 1);
}

#[test]
fn supersede_keeps_agent_record_newer_than_boundary() {
    let recs = [rec("s", "/p/proj", "Stop", BOUNDARY), sub("s", "ag1", "/p/proj", "PreToolUse", NEWER)];
    let s = summarize(&recs, NOW);
    assert_eq!(s.rows, vec![Row { id: "s".into(), label: "proj".into(), status: Status::Working, detail: None }]);
    assert_eq!((s.total, s.attention, s.working), (1, 0, 1));
}

#[test]
fn supersede_keeps_agent_record_with_equal_ts() {
    let recs = [rec("s", "/p/proj", "Stop", BOUNDARY), sub("s", "ag1", "/p/proj", "PreToolUse", BOUNDARY)];
    let s = summarize(&recs, NOW);
    assert_eq!(s.rows, vec![Row { id: "s".into(), label: "proj".into(), status: Status::Working, detail: None }]);
    assert_eq!((s.total, s.attention, s.working), (1, 0, 1));
}

#[test]
fn supersede_is_per_session() {
    let mut other = sub("t", "ag9", "/p/other", "PermissionRequest", STALE);
    other.tool_name = Some("Edit".into());
    let recs = [stale_perm(), rec("s", "/p/proj", "UserPromptSubmit", BOUNDARY), other];
    let s = summarize(&recs, NOW);
    assert_eq!(
        s.rows,
        vec![
            Row { id: "t".into(), label: "other".into(), status: Status::NeedsPermission, detail: Some("Edit".into()) },
            Row { id: "s".into(), label: "proj".into(), status: Status::Working, detail: None },
        ]
    );
    assert_eq!((s.total, s.attention, s.working), (2, 1, 1));
}

// next_wake (CONTRACT.md section 2). TTL_SECS = 10800, QUIET_SECS = 600; every expectation is a literal sum.

#[test]
fn next_wake_none_without_records() {
    assert_eq!(next_wake(&[], NOW), None);
}

#[test]
fn next_wake_none_for_ignored_events_only() {
    let recs = [rec("a", "/a", "SessionEnd", NOW - 10), rec("b", "/b", "SubagentStop", NOW - 20), rec("c", "/c", "Bogus", NOW)];
    assert_eq!(next_wake(&recs, NOW), None);
}

#[test]
fn next_wake_none_when_every_record_is_past_ttl() {
    let recs = [rec("a", "/a", "Stop", NOW - 10801), rec("b", "/b", "PreToolUse", NOW - 20000)];
    assert_eq!(next_wake(&recs, NOW), None);
}

#[test]
fn next_wake_working_session_wakes_at_decay_before_ttl() {
    assert_eq!(next_wake(&[rec("a", "/a", "PreToolUse", NOW - 100)], NOW), Some(NOW - 100 + 600 + 1));
}

#[test]
fn next_wake_decayed_working_session_wakes_at_ttl_expiry() {
    assert_eq!(next_wake(&[rec("a", "/a", "PreToolUse", NOW - 700)], NOW), Some(NOW - 700 + 10800 + 1));
}

#[test]
fn next_wake_non_working_sessions_wake_at_ttl_expiry_only() {
    let cases: [(Record, u64); 4] = [
        (rec("a", "/a", "Stop", NOW - 100), NOW - 100 + 10800 + 1),
        (rec("a", "/a", "SessionStart", NOW - 100), NOW - 100 + 10800 + 1),
        (rec("a", "/a", "PermissionRequest", NOW - 100), NOW - 100 + 10800 + 1),
        (rec("a", "/a", "StopFailure", NOW - 100), NOW - 100 + 10800 + 1),
    ];
    for (r, want) in cases {
        let event = r.event.clone();
        assert_eq!(next_wake(&[r], NOW), Some(want), "event {event}");
    }
    assert_eq!(next_wake(&[notif("elicitation_dialog")], NOW), Some(NOW + 10800 + 1));
}

#[test]
fn next_wake_age_exactly_quiet_secs_is_not_yet_decayed() {
    assert_eq!(next_wake(&[rec("a", "/a", "PreToolUse", NOW - 600)], NOW), Some(NOW + 1));
}

#[test]
fn next_wake_age_exactly_ttl_secs_is_still_kept() {
    assert_eq!(next_wake(&[rec("a", "/a", "Stop", NOW - 10800)], NOW), Some(NOW + 1));
}

#[test]
fn next_wake_decay_uses_the_groups_largest_ts() {
    // Working (priority) at NOW-500, a later Waiting record at NOW-20: the session ts is NOW-20.
    let recs = [rec("s", "/a", "PreToolUse", NOW - 500), rec("s", "/a", "SessionStart", NOW - 20)];
    assert_eq!(next_wake(&recs, NOW), Some(NOW - 20 + 600 + 1));
}

#[test]
fn next_wake_superseded_agent_has_no_decay_but_still_counts_for_ttl() {
    // The agent's Working record (age 550) is older than the main Stop and is dropped by superseding, so there is no
    // decay candidate; step 1 still keeps it, so its TTL expiry is the earliest candidate.
    let recs = [sub("s", "ag", "/a", "PreToolUse", NOW - 550), rec("s", "/a", "Stop", NOW - 10)];
    assert_eq!(next_wake(&recs, NOW), Some(NOW - 550 + 10800 + 1));
}

#[test]
fn next_wake_agent_newer_than_boundary_keeps_its_decay() {
    let recs = [sub("s", "ag", "/a", "PreToolUse", NOW - 10), rec("s", "/a", "Stop", NOW - 550)];
    assert_eq!(next_wake(&recs, NOW), Some(NOW - 10 + 600 + 1));
}

#[test]
fn next_wake_takes_the_minimum_across_sessions() {
    let recs = [
        rec("a", "/a", "Stop", NOW - 100),
        rec("b", "/b", "PreToolUse", NOW - 300),
        rec("c", "/c", "Stop", NOW - 10000),
    ];
    assert_eq!(next_wake(&recs, NOW), Some(NOW - 300 + 600 + 1));
    assert_eq!(next_wake(&recs[..1], NOW), Some(NOW - 100 + 10800 + 1));
    assert_eq!(next_wake(&[recs[0].clone(), recs[2].clone()], NOW), Some(NOW - 10000 + 10800 + 1));
}

#[test]
fn next_wake_future_ts_keeps_candidates_after_now() {
    assert_eq!(next_wake(&[rec("a", "/a", "PreToolUse", NOW + 50)], NOW), Some(NOW + 50 + 600 + 1));
    assert_eq!(next_wake(&[rec("a", "/a", "Stop", NOW + 50)], NOW), Some(NOW + 50 + 10800 + 1));
}

#[test]
fn next_wake_is_always_after_now() {
    let sets: Vec<Vec<Record>> = vec![
        vec![rec("a", "/a", "PreToolUse", NOW)],
        vec![rec("a", "/a", "PreToolUse", NOW - 600)],
        vec![rec("a", "/a", "PreToolUse", NOW - 601)],
        vec![rec("a", "/a", "Stop", NOW - 10800)],
        vec![rec("a", "/a", "Stop", NOW + 1000)],
        vec![rec("a", "/a", "Stop", NOW - 5), rec("b", "/b", "PreToolUse", NOW - 599)],
        vec![sub("s", "ag", "/a", "PreToolUse", NOW - 550), rec("s", "/a", "Stop", NOW - 10)],
    ];
    for recs in &sets {
        for now in [NOW - 3, NOW, NOW + 1, NOW + 599, NOW + 601, NOW + 5000] {
            if let Some(w) = next_wake(recs, now) {
                assert!(w > now, "wake {w} must be after now {now} for {recs:?}");
            }
        }
    }
}

#[test]
fn next_wake_summary_is_stable_until_the_wake_second() {
    let sets: Vec<Vec<Record>> = vec![
        vec![rec("a", "/a", "Stop", NOW - 100)],
        vec![rec("a", "/a", "Stop", NOW - 10790), rec("b", "/b", "PreToolUse", NOW - 50)],
        vec![sub("s", "ag", "/a", "PreToolUse", NOW - 550), rec("s", "/a", "Stop", NOW - 10)],
        vec![rec("s", "/a", "PreToolUse", NOW - 500), rec("s", "/a", "SessionStart", NOW - 20)],
    ];
    for recs in &sets {
        let wake = next_wake(recs, NOW).expect("a kept record gives a wake");
        assert_eq!(summarize(recs, wake - 1), summarize(recs, NOW), "unchanged just before the wake for {recs:?}");
    }
}

#[test]
fn next_wake_decay_second_flips_working_to_waiting() {
    let recs = [rec("a", "/a", "PreToolUse", NOW - 100)];
    let wake = next_wake(&recs, NOW).unwrap();
    assert_eq!(wake, NOW + 501);
    let before = summarize(&recs, wake - 1);
    let at = summarize(&recs, wake);
    assert_eq!(before.rows[0].status, Status::Working);
    assert_eq!(at.rows[0].status, Status::Waiting);
    assert_ne!(before, at);
}

#[test]
fn next_wake_ttl_second_drops_the_session() {
    let recs = [rec("a", "/a", "Stop", NOW - 100)];
    let wake = next_wake(&recs, NOW).unwrap();
    assert_eq!(summarize(&recs, wake - 1).total, 1);
    assert_eq!(summarize(&recs, wake).total, 0);
}

// valid_id and detail (CONTRACT.md section 2, "Session ids and detail"). Expectations are literals.

#[test]
fn valid_id_accepts_the_hook_alphabet() {
    for id in ["abc", "A-z_09", "0", "-", "_"] {
        assert!(valid_id(id), "{id:?} should be valid");
    }
}

#[test]
fn valid_id_rejects_everything_else() {
    for id in ["", "a b", "a/b", "a.b", "a\n", "\na", "é", "aé", "a\0", " "] {
        assert!(!valid_id(id), "{id:?} should be invalid");
    }
}

fn with_tool(mut r: Record, tool: &str) -> Record {
    r.tool_name = Some(tool.to_string());
    r
}

#[test]
fn detail_of_a_plain_session() {
    let recs = [with_tool(rec("sid1", "/Users/lem/proj", "PreToolUse", NOW - 5), "Bash")];
    assert_eq!(
        detail(&recs, "sid1", NOW),
        Some(Detail {
            id: "sid1".into(),
            label: "proj".into(),
            cwd: "/Users/lem/proj".into(),
            status: Status::Working,
            event: "PreToolUse".into(),
            event_ts: NOW - 5,
            tool_name: Some("Bash".into()),
            agents: 0,
            reach: crate::prompt::Reach::None,
            app: None,
        })
    );
}

#[test]
fn detail_of_a_session_with_an_inbox_reaches_it() {
    let r = Record { inbox: Some("/tmp/claude-1/501/7.sock".into()), ..rec("sid1", "/Users/lem/proj", "PreToolUse", NOW - 5) };
    let d = detail(&[r], "sid1", NOW).unwrap();
    assert_eq!(d.reach, crate::prompt::Reach::Inbox);
}

#[test]
fn detail_unknown_id_is_none() {
    let recs = [rec("sid1", "/p/proj", "Stop", NOW)];
    assert_eq!(detail(&recs, "nope", NOW), None);
    assert_eq!(detail(&recs, "", NOW), None);
    assert_eq!(detail(&[], "sid1", NOW), None);
}

#[test]
fn detail_expired_session_is_none() {
    let recs = [rec("old", "/p/old", "Stop", NOW - TTL_SECS - 1), rec("new", "/p/new", "Stop", NOW - TTL_SECS)];
    assert_eq!(detail(&recs, "old", NOW), None);
    assert!(detail(&recs, "new", NOW).is_some());
}

#[test]
fn detail_session_with_only_ignored_events_is_none() {
    let recs = [rec("sid1", "/p/proj", "SessionEnd", NOW), rec("sid1", "/p/proj", "SubagentStop", NOW)];
    assert_eq!(detail(&recs, "sid1", NOW), None);
}

#[test]
fn detail_newest_record_gives_event_ts_and_tool() {
    let recs = [
        with_tool(rec("s", "/p/proj", "PreToolUse", NOW - 30), "Read"),
        with_tool(sub("s", "ag1", "/p/proj", "PostToolUse", NOW - 3), "Grep"),
        with_tool(rec("s", "/p/proj", "PostToolUse", NOW - 20), "Edit"),
    ];
    let d = detail(&recs, "s", NOW).unwrap();
    assert_eq!((d.event.as_str(), d.event_ts, d.tool_name), ("PostToolUse", NOW - 3, Some("Grep".into())));
    assert_eq!(d.agents, 1);
}

#[test]
fn detail_status_comes_from_priority_but_event_from_newest() {
    let recs = [
        with_tool(rec("s", "/p/proj", "PermissionRequest", NOW - 20), "Edit"),
        with_tool(sub("s", "ag1", "/p/proj", "PreToolUse", NOW - 1), "Read"),
    ];
    let d = detail(&recs, "s", NOW).unwrap();
    assert_eq!(d.status, Status::NeedsPermission);
    assert_eq!((d.event.as_str(), d.event_ts, d.tool_name), ("PreToolUse", NOW - 1, Some("Read".into())));
    assert_eq!(d.agents, 1);
}

#[test]
fn detail_tie_prefers_the_main_thread() {
    let recs = [
        with_tool(sub("s", "ag1", "/p/proj", "PreToolUse", NOW - 4), "Grep"),
        with_tool(rec("s", "/p/proj", "PostToolUse", NOW - 4), "Edit"),
    ];
    let d = detail(&recs, "s", NOW).unwrap();
    assert_eq!((d.event.as_str(), d.event_ts, d.tool_name), ("PostToolUse", NOW - 4, Some("Edit".into())));
    assert_eq!(d.agents, 1);
}

#[test]
fn detail_tie_between_agents_takes_the_smallest_agent_id() {
    let recs = [
        with_tool(sub("s", "ag3", "/p/proj", "PreToolUse", NOW - 4), "Three"),
        with_tool(sub("s", "ag1", "/p/proj", "PostToolUse", NOW - 4), "One"),
        with_tool(sub("s", "ag2", "/p/proj", "PreToolUse", NOW - 4), "Two"),
    ];
    let d = detail(&recs, "s", NOW).unwrap();
    assert_eq!((d.event.as_str(), d.tool_name), ("PostToolUse", Some("One".into())));
    assert_eq!(d.agents, 3);
}

#[test]
fn detail_ignores_superseded_subagents() {
    // The main thread's Stop (BOUNDARY) clears the older agent record (STALE); it is neither counted nor newest.
    let recs = [
        rec("s", "/p/proj", "Stop", BOUNDARY),
        with_tool(sub("s", "ag1", "/p/proj", "PreToolUse", STALE), "Bash"),
    ];
    let d = detail(&recs, "s", NOW).unwrap();
    assert_eq!(d.status, Status::Done);
    assert_eq!((d.event.as_str(), d.event_ts, d.tool_name, d.agents), ("Stop", BOUNDARY, None, 0));
}

#[test]
fn detail_agents_counts_live_subagent_records_only() {
    let recs = [
        rec("s", "/p/proj", "PreToolUse", NOW - 1),
        sub("s", "ag1", "/p/proj", "PreToolUse", NOW - 2),
        sub("s", "ag2", "/p/proj", "PostToolUse", NOW - 3),
        sub("s", "ag3", "/p/proj", "SubagentStop", NOW - 1),
        sub("other", "ag9", "/p/other", "PreToolUse", NOW),
    ];
    assert_eq!(detail(&recs, "s", NOW).unwrap().agents, 2);
}

#[test]
fn detail_ignored_events_are_not_the_newest() {
    let recs = [rec("s", "/p/proj", "SessionEnd", NOW), rec("s", "/p/proj", "Stop", NOW - 9)];
    let d = detail(&recs, "s", NOW).unwrap();
    assert_eq!((d.event.as_str(), d.event_ts), ("Stop", NOW - 9));
}

#[test]
fn detail_working_decays_to_waiting_after_the_quiet_window() {
    let at = |age: u64| detail(&[rec("s", "/p/proj", "PreToolUse", NOW - age)], "s", NOW).unwrap();
    assert_eq!(at(600).status, Status::Working);
    let late = at(601);
    assert_eq!(late.status, Status::Waiting);
    assert_eq!((late.event.as_str(), late.event_ts), ("PreToolUse", NOW - 601));
}

#[test]
fn detail_cwd_comes_from_the_main_thread_record() {
    let recs = [
        rec("s", "/main/dir", "Stop", NOW - 30),
        with_tool(sub("s", "ag1", "/agent/dir", "PermissionRequest", NOW - 1), "Bash"),
    ];
    let d = detail(&recs, "s", NOW).unwrap();
    assert_eq!((d.cwd.as_str(), d.label.as_str()), ("/main/dir", "dir"));
    assert_eq!(d.status, Status::NeedsPermission);
}

#[test]
fn detail_cwd_without_a_main_record_comes_from_the_status_record() {
    let recs = [
        sub("s", "ag1", "/p/quiet", "Stop", NOW),
        sub("s", "ag2", "/p/loud", "PermissionRequest", NOW - 10),
    ];
    let d = detail(&recs, "s", NOW).unwrap();
    assert_eq!((d.cwd.as_str(), d.label.as_str()), ("/p/loud", "loud"));
}

#[test]
fn detail_label_has_the_suffix_when_two_sessions_share_a_folder() {
    let recs = [
        rec("aaaa1111", "/one/app", "Stop", NOW),
        rec("bbbb2222", "/two/app", "Stop", NOW - 1),
        rec("cccc3333", "/three/other", "Stop", NOW - 2),
    ];
    assert_eq!(detail(&recs, "aaaa1111", NOW).unwrap().label, "app · aaaa");
    assert_eq!(detail(&recs, "bbbb2222", NOW).unwrap().label, "app · bbbb");
    assert_eq!(detail(&recs, "cccc3333", NOW).unwrap().label, "other");
}

#[test]
fn detail_label_counts_expired_sessions_out() {
    let recs = [rec("aaaa1111", "/one/app", "Stop", NOW), rec("bbbb2222", "/two/app", "Stop", NOW - TTL_SECS - 1)];
    assert_eq!(detail(&recs, "aaaa1111", NOW).unwrap().label, "app");
}

#[test]
fn detail_exists_for_a_session_past_the_row_cap() {
    let recs: Vec<Record> = (0..6u64).map(|i| rec(&format!("w{i}"), &format!("/p/w{i}"), "PreToolUse", NOW - i)).collect();
    let s = summarize(&recs, NOW);
    assert_eq!(s.rows.len(), 4);
    let d = detail(&recs, "w5", NOW).unwrap();
    assert_eq!((d.label.as_str(), d.event_ts), ("w5", NOW - 5));
}

#[test]
fn row_ids_are_the_session_ids() {
    let recs = [rec("sess-1", "/p/a", "Stop", NOW), rec("sess_2", "/p/b", "PreToolUse", NOW)];
    let ids: Vec<String> = summarize(&recs, NOW).rows.into_iter().map(|r| r.id).collect();
    assert_eq!(ids, vec!["sess_2".to_string(), "sess-1".to_string()]);
}

#[test]
fn detail_serializes_with_a_null_app() {
    let d = Detail {
        id: "s1".to_string(),
        label: "proj".to_string(),
        cwd: "/x/proj".to_string(),
        status: Status::Working,
        event: "PreToolUse".to_string(),
        event_ts: 100,
        tool_name: Some("Read".to_string()),
        agents: 2,
        reach: Reach::Inbox,
        app: None,
    };
    assert_eq!(
        serde_json::to_string(&d).unwrap(),
        r#"{"id":"s1","label":"proj","cwd":"/x/proj","status":"working","event":"PreToolUse","event_ts":100,"tool_name":"Read","agents":2,"reach":{"kind":"inbox"},"app":null}"#
    );
}

#[test]
fn detail_serializes_a_host_only_app() {
    let d = Detail {
        id: "s1".to_string(),
        label: "proj".to_string(),
        cwd: "/x/proj".to_string(),
        status: Status::Working,
        event: "PreToolUse".to_string(),
        event_ts: 100,
        tool_name: None,
        agents: 0,
        reach: Reach::HostOnly("com.apple.TextEdit".to_string()),
        app: Some("TextEdit".to_string()),
    };
    assert_eq!(
        serde_json::to_string(&d).unwrap(),
        r#"{"id":"s1","label":"proj","cwd":"/x/proj","status":"working","event":"PreToolUse","event_ts":100,"tool_name":null,"agents":0,"reach":{"kind":"host_only","host":"com.apple.TextEdit"},"app":"TextEdit"}"#
    );
}
