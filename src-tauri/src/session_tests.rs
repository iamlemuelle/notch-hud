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
        vec![Row { label: "keep".into(), status: Status::Done, detail: None }]
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
            ("work".to_string(), None),
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
            Status::Working,
            Status::Done,
            Status::Waiting
        ]
    );
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
    assert_eq!(labels, vec!["p0", "p1", "p2", "n0", "w0", "w1"]);
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
            rows: vec![Row { label: "proj".into(), status: Status::NeedsPermission, detail: Some("Bash".into()) }],
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
        vec![Row { label: "proj".into(), status: Status::NeedsPermission, detail: Some("Edit".into()) }]
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
    assert_eq!(s.rows, vec![Row { label: "proj".into(), status: Status::Working, detail: None }]);
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
    assert_eq!(s.rows, vec![Row { label: "proj".into(), status: Status::Waiting, detail: None }]);
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
fn contract_summarize_caps_at_6_rows() {
    let recs: Vec<Record> = (0..7u64)
        .map(|i| rec(&format!("r{i}"), &format!("/p/r{i}"), "Stop", NOW - i))
        .collect();
    let s = summarize(&recs, NOW);
    assert_eq!(s.total, 7);
    assert_eq!(s.rows.len(), 6);
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
