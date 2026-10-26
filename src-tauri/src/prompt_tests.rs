use super::*;
use std::fs;
use std::io::Read;
use std::os::unix::net::UnixListener;

fn rec(id: &str, ts: u64, inbox: Option<&str>, host: Option<&str>) -> Record {
    Record {
        session_id: id.to_string(),
        cwd: "/x/proj".to_string(),
        event: "PreToolUse".to_string(),
        agent_id: None,
        notification_type: None,
        tool_name: None,
        ts,
        inbox: inbox.map(String::from),
        host: host.map(String::from),
    }
}

fn agent(id: &str, ts: u64, inbox: Option<&str>) -> Record {
    Record { agent_id: Some("a1".to_string()), ..rec(id, ts, inbox, None) }
}

/// A fresh directory under /tmp, short enough for a socket path.
fn tmp(name: &str) -> PathBuf {
    let d = PathBuf::from(format!("/tmp/nh-pt-{}-{}", std::process::id(), name));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    d
}

fn absolute(len: usize) -> String {
    format!("/{}", "a".repeat(len - 1))
}

const SOCK: &str = "/tmp/claude-501/7.sock";

// parse_prompt ---------------------------------------------------------------------------------------------------

#[test]
fn parse_rejects_empty_and_whitespace_text() {
    assert_eq!(parse_prompt("s1", ""), Err("empty text".to_string()));
    assert_eq!(parse_prompt("s1", "   "), Err("empty text".to_string()));
    assert_eq!(parse_prompt("s1", " \n\t \r\n "), Err("empty text".to_string()));
}

#[test]
fn parse_counts_scalar_values_at_the_limit() {
    assert!(parse_prompt("s1", &"a".repeat(8000)).is_ok());
    assert_eq!(parse_prompt("s1", &"a".repeat(8001)), Err("text is 8001 chars, over 8000".to_string()));
    // A four-byte scalar counts once: 8000 of them are 32000 bytes and pass; 8001 do not.
    assert!(parse_prompt("s1", &"\u{1F600}".repeat(8000)).is_ok());
    assert_eq!(parse_prompt("s1", &"\u{1F600}".repeat(8001)), Err("text is 8001 chars, over 8000".to_string()));
}

#[test]
fn parse_counts_the_trimmed_text() {
    let padded = format!("  {}  ", "a".repeat(8000));
    assert_eq!(parse_prompt("s1", &padded).unwrap().text, "a".repeat(8000));
}

#[test]
fn parse_rejects_nul() {
    assert_eq!(parse_prompt("s1", "he\0llo"), Err("text contains NUL".to_string()));
}

#[test]
fn parse_rejects_bad_ids() {
    for id in ["", "a b", "a\n"] {
        assert_eq!(parse_prompt(id, "hi"), Err("bad session id".to_string()), "{id:?}");
    }
}

#[test]
fn parse_trims_and_keeps_inner_newlines() {
    assert_eq!(
        parse_prompt("sess-1", "  \n first line\nsecond\n\nthird \t\n"),
        Ok(Prompt { id: "sess-1".to_string(), text: "first line\nsecond\n\nthird".to_string() })
    );
}

// route ----------------------------------------------------------------------------------------------------------

#[test]
fn route_goes_to_the_inbox_of_the_main_record() {
    let recs = [rec("s1", 100, Some(SOCK), None)];
    assert_eq!(route(&recs, "s1"), Ok(Route::Inbox(PathBuf::from("/tmp/claude-501/7.sock"))));
}

#[test]
fn route_refuses_a_relative_path() {
    let recs = [rec("s1", 100, Some("claude/7.sock"), None)];
    assert_eq!(route(&recs, "s1"), Err("inbox path is not absolute".to_string()));
}

#[test]
fn route_refuses_a_104_byte_path_and_accepts_103() {
    let long = absolute(104);
    let ok = absolute(103);
    assert_eq!(long.len(), 104);
    assert_eq!(ok.len(), 103);
    assert_eq!(
        route(&[rec("s1", 100, Some(&long), None)], "s1"),
        Err("inbox path is 104 bytes, the limit is 103".to_string())
    );
    assert_eq!(route(&[rec("s1", 100, Some(&ok), None)], "s1"), Ok(Route::Inbox(PathBuf::from(&ok))));
}

#[test]
fn route_needs_a_main_thread_record() {
    let none = Err("no record for this session".to_string());
    assert_eq!(route(&[], "s1"), none);
    assert_eq!(route(&[rec("other", 100, Some(SOCK), None)], "s1"), none);
    assert_eq!(route(&[agent("s1", 100, Some(SOCK))], "s1"), none);
}

#[test]
fn route_refuses_a_null_and_an_empty_inbox() {
    let none = Err("no inbox in the record".to_string());
    assert_eq!(route(&[rec("s1", 100, None, None)], "s1"), none);
    assert_eq!(route(&[rec("s1", 100, Some(""), None)], "s1"), none);
}

#[test]
fn route_takes_the_newest_main_record() {
    let recs = [rec("s1", 100, Some("/tmp/old.sock"), None), rec("s1", 200, Some("/tmp/new.sock"), None)];
    assert_eq!(route(&recs, "s1"), Ok(Route::Inbox(PathBuf::from("/tmp/new.sock"))));
}

#[test]
fn route_a_subagent_of_the_same_session_is_not_sharing() {
    let recs = [rec("s1", 100, Some(SOCK), None), agent("s1", 100, Some(SOCK)), agent("s1", 300, Some(SOCK))];
    assert_eq!(route(&recs, "s1"), Ok(Route::Inbox(PathBuf::from(SOCK))));
}

#[test]
fn route_refuses_a_shared_socket_on_a_tie() {
    let recs = [rec("s1", 100, Some(SOCK), None), rec("s2", 100, Some(SOCK), None)];
    assert_eq!(route(&recs, "s1"), Err("inbox is shared with a session as new or newer".to_string()));
    assert_eq!(route(&recs, "s2"), Err("inbox is shared with a session as new or newer".to_string()));
}

#[test]
fn route_refuses_a_shared_socket_when_the_other_is_newer() {
    let recs = [rec("s1", 100, Some(SOCK), None), rec("s2", 101, Some(SOCK), None)];
    assert_eq!(route(&recs, "s1"), Err("inbox is shared with a session as new or newer".to_string()));
}

#[test]
fn route_sends_a_shared_socket_to_the_newest_session() {
    let recs = [rec("s1", 100, Some(SOCK), None), rec("s2", 101, Some(SOCK), None)];
    assert_eq!(route(&recs, "s2"), Ok(Route::Inbox(PathBuf::from(SOCK))));
}

#[test]
fn route_counts_the_sessions_own_newer_subagent_against_the_other() {
    let recs = [rec("s1", 100, Some(SOCK), None), agent("s1", 300, Some(SOCK)), rec("s2", 200, Some(SOCK), None)];
    assert_eq!(route(&recs, "s1"), Ok(Route::Inbox(PathBuf::from(SOCK))));
    assert_eq!(route(&recs, "s2"), Err("inbox is shared with a session as new or newer".to_string()));
}

#[test]
fn route_ignores_other_sessions_on_other_sockets() {
    let recs = [rec("s1", 100, Some(SOCK), None), rec("s2", 500, Some("/tmp/claude-501/8.sock"), None)];
    assert_eq!(route(&recs, "s1"), Ok(Route::Inbox(PathBuf::from(SOCK))));
}

// reach ----------------------------------------------------------------------------------------------------------

#[test]
fn reach_inbox_when_routable() {
    assert_eq!(reach(&[rec("s1", 100, Some(SOCK), Some("com.apple.Terminal"))], "s1"), Reach::Inbox);
}

#[test]
fn reach_host_only_without_an_inbox() {
    assert_eq!(
        reach(&[rec("s1", 100, None, Some("com.apple.Terminal"))], "s1"),
        Reach::HostOnly("com.apple.Terminal".to_string())
    );
    assert_eq!(
        reach(&[rec("s1", 100, Some(""), Some("com.apple.Terminal"))], "s1"),
        Reach::HostOnly("com.apple.Terminal".to_string())
    );
}

#[test]
fn reach_host_only_when_the_inbox_is_unroutable() {
    let recs = [rec("s1", 100, Some("rel.sock"), Some("com.anthropic.claudefordesktop"))];
    assert_eq!(reach(&recs, "s1"), Reach::HostOnly("com.anthropic.claudefordesktop".to_string()));
}

#[test]
fn reach_none_with_neither() {
    assert_eq!(reach(&[rec("s1", 100, None, None)], "s1"), Reach::None);
    assert_eq!(reach(&[rec("s1", 100, None, Some(""))], "s1"), Reach::None);
    assert_eq!(reach(&[], "s1"), Reach::None);
}

#[test]
fn reach_serializes_as_the_contract_writes_it() {
    assert_eq!(serde_json::to_string(&Reach::Inbox).unwrap(), r#"{"kind":"inbox"}"#);
    assert_eq!(
        serde_json::to_string(&Reach::HostOnly("com.apple.Terminal".to_string())).unwrap(),
        r#"{"kind":"host_only","host":"com.apple.Terminal"}"#
    );
    assert_eq!(serde_json::to_string(&Reach::None).unwrap(), r#"{"kind":"none"}"#);
}

#[test]
fn bundle_id_accepts_letters_digits_dash_and_dots() {
    for id in [
        "com.apple.TextEdit",
        "com.anthropic.claudefordesktop",
        "com.googlecode.iterm2",
        "com.mitchellh.ghostty",
        "a",
        "A-1.b",
    ] {
        assert!(valid_bundle_id(id), "{id:?}");
    }
    assert!(valid_bundle_id(&"a".repeat(255)));
}

#[test]
fn bundle_id_rejects_everything_else() {
    for id in ["", ".a", "a.", "a..b", "a b", "a/b", "a_b", "com.x\n", "com.x\0y", "c\u{f3}m.x"] {
        assert!(!valid_bundle_id(id), "{id:?}");
    }
    assert!(!valid_bundle_id(&"a".repeat(256)));
}

#[test]
fn reach_host_only_needs_a_valid_bundle_id() {
    assert_eq!(
        reach(&[rec("s1", 100, None, Some("com.apple.TextEdit"))], "s1"),
        Reach::HostOnly("com.apple.TextEdit".to_string())
    );
    assert_eq!(reach(&[rec("s1", 100, None, Some("com.apple.Text Edit"))], "s1"), Reach::None);
    assert_eq!(reach(&[rec("s1", 100, None, Some("../x"))], "s1"), Reach::None);
}

#[test]
fn reach_inbox_wins_over_an_invalid_host() {
    assert_eq!(reach(&[rec("s1", 100, Some(SOCK), Some("../x"))], "s1"), Reach::Inbox);
}

#[test]
fn host_open_serializes_as_the_contract_writes_it() {
    assert_eq!(
        serde_json::to_string(&HostOpen::Opened("TextEdit".to_string())).unwrap(),
        r#"{"kind":"opened","name":"TextEdit"}"#
    );
    assert_eq!(serde_json::to_string(&HostOpen::NotRunning).unwrap(), r#"{"kind":"not_running"}"#);
}

// frame ----------------------------------------------------------------------------------------------------------

#[test]
fn frame_plain_text() {
    assert_eq!(frame("hello"), "{\"type\":\"user\",\"message\":{\"role\":\"user\",\"content\":\"hello\"}}\n");
}

#[test]
fn frame_escapes_quotes_and_backslashes() {
    assert_eq!(
        frame("say \"hi\" \\ bye"),
        "{\"type\":\"user\",\"message\":{\"role\":\"user\",\"content\":\"say \\\"hi\\\" \\\\ bye\"}}\n"
    );
}

#[test]
fn frame_escapes_newlines_and_tabs() {
    assert_eq!(
        frame("a\nb\tc"),
        "{\"type\":\"user\",\"message\":{\"role\":\"user\",\"content\":\"a\\nb\\tc\"}}\n"
    );
}

#[test]
fn frame_keeps_an_emoji_as_utf8() {
    assert_eq!(
        frame("ok \u{1F600}"),
        "{\"type\":\"user\",\"message\":{\"role\":\"user\",\"content\":\"ok \u{1F600}\"}}\n"
    );
    assert!(!frame("\u{1F600}").contains("\\u"));
}

#[test]
fn frame_ends_in_exactly_one_newline() {
    for text in ["hello", "a\nb", "tail\n"] {
        let line = frame(text);
        assert!(line.ends_with("}}\n"), "{line:?}");
        assert_eq!(line.matches('\n').count(), 1, "{line:?}");
    }
}

// send -----------------------------------------------------------------------------------------------------------

#[test]
fn send_delivers_exactly_the_line() {
    let dir = tmp("deliver");
    let sock = dir.join("i.sock");
    let listener = UnixListener::bind(&sock).unwrap();
    let reader = std::thread::spawn(move || {
        let (mut conn, _) = listener.accept().unwrap();
        let mut got = Vec::new();
        conn.read_to_end(&mut got).unwrap();
        got
    });
    let line = "{\"type\":\"user\",\"message\":{\"role\":\"user\",\"content\":\"hello\"}}\n";
    assert_eq!(send(&sock, line), Ok(()));
    assert_eq!(reader.join().unwrap(), line.as_bytes());
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn send_rejects_a_regular_file() {
    let dir = tmp("regular");
    let file = dir.join("i.sock");
    fs::write(&file, b"not a socket").unwrap();
    assert_eq!(send(&file, "x\n"), Err(SendError::Rejected("inbox is not a socket".to_string())));
    assert_eq!(fs::read(&file).unwrap(), b"not a socket");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn send_rejects_a_symlink_to_a_live_socket() {
    let dir = tmp("symlink");
    let sock = dir.join("real.sock");
    let link = dir.join("link.sock");
    let listener = UnixListener::bind(&sock).unwrap();
    listener.set_nonblocking(true).unwrap();
    std::os::unix::fs::symlink(&sock, &link).unwrap();
    assert_eq!(send(&link, "x\n"), Err(SendError::Rejected("inbox is a symlink".to_string())));
    // Nothing connected: the live listener saw no one.
    assert_eq!(listener.accept().unwrap_err().kind(), std::io::ErrorKind::WouldBlock);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn send_rejects_a_missing_path() {
    let dir = tmp("missing");
    match send(&dir.join("none.sock"), "x\n") {
        Err(SendError::Rejected(why)) => assert!(why.starts_with("lstat: "), "{why}"),
        other => panic!("{other:?}"),
    }
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn send_fails_when_the_listener_is_gone() {
    let dir = tmp("gone");
    let sock = dir.join("i.sock");
    drop(UnixListener::bind(&sock).unwrap());
    assert!(sock.exists());
    match send(&sock, "x\n") {
        Err(SendError::Failed(why)) => assert!(why.starts_with("connect: "), "{why}"),
        other => panic!("{other:?}"),
    }
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn check_owner_passes_the_same_uid() {
    assert_eq!(check_owner("inbox", 501, 501), Ok(()));
    assert_eq!(check_owner("inbox listener", 0, 0), Ok(()));
}

#[test]
fn check_owner_rejects_another_uid_naming_both() {
    assert_eq!(check_owner("inbox", 0, 501), Err(SendError::Rejected("inbox belongs to uid 0, not 501".to_string())));
    assert_eq!(
        check_owner("inbox listener", 502, 501),
        Err(SendError::Rejected("inbox listener belongs to uid 502, not 501".to_string()))
    );
}

#[test]
fn send_to_a_live_listener_of_our_uid_passes_the_peer_check_and_delivers_a_large_line() {
    let dir = tmp("peer");
    let sock = dir.join("i.sock");
    let listener = UnixListener::bind(&sock).unwrap();
    let reader = std::thread::spawn(move || {
        let (mut conn, _) = listener.accept().unwrap();
        let mut got = Vec::new();
        conn.read_to_end(&mut got).unwrap();
        got
    });
    let line = format!("{}\n", "x".repeat(300_000));
    assert_eq!(send(&sock, &line), Ok(()));
    assert_eq!(reader.join().unwrap(), line.as_bytes());
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn send_gives_up_on_a_listener_that_never_reads_within_one_timeout_for_the_whole_line() {
    let dir = tmp("stall");
    let sock = dir.join("i.sock");
    let listener = UnixListener::bind(&sock).unwrap();
    let (done_tx, done_rx) = std::sync::mpsc::channel::<()>();
    // Accepts, never reads, and stays alive until send has returned.
    let holder = std::thread::spawn(move || {
        let (conn, _) = listener.accept().unwrap();
        let _ = done_rx.recv();
        drop(conn);
    });
    let line = "y".repeat(800_000);
    let start = Instant::now();
    let result = send(&sock, &line);
    let took = start.elapsed();
    done_tx.send(()).unwrap();
    holder.join().unwrap();
    match result {
        Err(SendError::Failed(why)) => {
            assert!(why.starts_with("write"), "{why}");
            assert!(why.ends_with("of 800000 bytes"), "{why}");
        }
        other => panic!("{other:?}"),
    }
    assert!(took >= Duration::from_millis(1500), "{took:?}");
    assert!(took < Duration::from_millis(3500), "{took:?}");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn send_reports_a_hard_write_error_at_once_instead_of_retrying_to_the_deadline() {
    let dir = tmp("epipe");
    let sock = dir.join("i.sock");
    let listener = UnixListener::bind(&sock).unwrap();
    // Accepts, lets the sender fill the socket buffer and block inside one write, then drops its end. Rust ignores
    // SIGPIPE, so the blocked write returns EPIPE.
    let closer = std::thread::spawn(move || {
        let (conn, _) = listener.accept().unwrap();
        std::thread::sleep(Duration::from_millis(300));
        drop(conn);
    });
    let line = "z".repeat(4_000_000);
    let start = Instant::now();
    let result = send(&sock, &line);
    let took = start.elapsed();
    closer.join().unwrap();
    match result {
        Err(SendError::Failed(why)) => {
            assert!(why.starts_with("write: "), "{why}");
            assert!(why.ends_with("of 4000000 bytes"), "{why}");
        }
        other => panic!("{other:?}"),
    }
    assert!(took < Duration::from_millis(1500), "{took:?}");
    let _ = fs::remove_dir_all(&dir);
}
