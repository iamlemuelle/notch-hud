use super::*;
use std::fs;
use std::path::PathBuf;

const NOW: u64 = 1_800_000_000;

fn rec(id: &str, cwd: &str, event: &str, ts: u64) -> Record {
    Record {
        session_id: id.to_string(),
        cwd: cwd.to_string(),
        event: event.to_string(),
        notification_type: None,
        tool_name: None,
        ts,
    }
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
        ("Stop", Some(Status::Done)),
        ("StopFailure", Some(Status::Error)),
        ("SessionEnd", None),
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
fn read_dir_missing_returns_empty() {
    let d = std::env::temp_dir().join(format!("notch-session-tests-{}-missing", std::process::id()));
    let _ = fs::remove_dir_all(&d);
    assert_eq!(read_dir(&d), Vec::<Record>::new());
}

#[test]
fn read_dir_parses_skips_and_sorts() {
    let d = tmp("readdir");
    fs::write(d.join("b.json"), json_for("b")).unwrap();
    fs::write(d.join("a.json"), json_for("a")).unwrap();
    fs::write(d.join("bad.json"), "{not json").unwrap();
    fs::write(d.join("notes.txt"), json_for("txt")).unwrap();
    fs::write(d.join("c.json.tmp"), json_for("tmp")).unwrap();
    let got = read_dir(&d);
    assert_eq!(
        got,
        vec![
            Record {
                session_id: "a".into(),
                cwd: "/x/a".into(),
                event: "Stop".into(),
                notification_type: None,
                tool_name: None,
                ts: 100
            },
            Record {
                session_id: "b".into(),
                cwd: "/x/b".into(),
                event: "Stop".into(),
                notification_type: None,
                tool_name: None,
                ts: 100
            },
        ]
    );
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
    assert_eq!(got[0].tool_name.as_deref(), Some("Bash"));
    assert_eq!(got[0].notification_type, None);
    assert_eq!(got[1].notification_type.as_deref(), Some("idle_prompt"));
    assert_eq!(got[1].tool_name, None);
    let _ = fs::remove_dir_all(&d);
}

#[test]
fn read_dir_caps_at_max_files_by_name() {
    let d = tmp("maxfiles");
    for i in 0..(MAX_FILES + 10) {
        let id = format!("s{i:03}");
        fs::write(d.join(format!("{id}.json")), json_for(&id)).unwrap();
    }
    let got = read_dir(&d);
    assert_eq!(got.len(), MAX_FILES);
    assert_eq!(got[0].session_id, "s000");
    assert_eq!(got[MAX_FILES - 1].session_id, format!("s{:03}", MAX_FILES - 1));
    let _ = fs::remove_dir_all(&d);
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
