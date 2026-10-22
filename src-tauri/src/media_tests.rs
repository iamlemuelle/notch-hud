use super::*;
use std::time::Duration;

const STREAM: &str = include_str!("../../tests/media/fixtures/adapter-stream.jsonl");

fn fixture_line(n: usize) -> &'static str {
    STREAM.lines().nth(n).expect("fixture line")
}

fn adapter(payload: &str) -> String {
    format!(r#"{{"type":"data","diff":false,"payload":{payload}}}"#)
}

// ---- parse_adapter_line: recorded fixture ----

#[test]
fn fixture_has_seven_lines() {
    assert_eq!(STREAM.lines().count(), 7);
}

#[test]
fn fixture_empty_payload_lines_are_none() {
    for n in [0, 3, 6] {
        assert_eq!(parse_adapter_line(fixture_line(n)), Ok(None), "line {n}");
    }
}

#[test]
fn fixture_first_track_without_artwork() {
    let t = parse_adapter_line(fixture_line(1)).unwrap().unwrap();
    assert_eq!(
        t,
        Track {
            title: "Low Tide Static".into(),
            artist: Some("Mara Venn".into()),
            album: Some("Fixture Album".into()),
            app: "dev.lem.notchhud.fixture".into(),
            bundle_id: "dev.lem.notchhud.fixture".into(),
            playing: true,
            duration_ms: Some(223_000),
            elapsed_ms: Some(74_000),
            elapsed_at_ms: Some(1_792_597_406_268),
            artwork: None,
        }
    );
}

#[test]
fn fixture_first_track_with_artwork() {
    let t = parse_adapter_line(fixture_line(2)).unwrap().unwrap();
    assert_eq!(t.title, "Low Tide Static");
    assert_eq!(t.artist.as_deref(), Some("Mara Venn"));
    assert_eq!(t.elapsed_ms, Some(74_000));
    assert_eq!(t.elapsed_at_ms, Some(1_792_597_406_268));
    assert_eq!(t.duration_ms, Some(223_000));
    let art = t.artwork.expect("artwork");
    assert!(art.starts_with("data:image/jpeg;base64,/9j/4AAQSkZJRgABAQAASABI"));
    assert_eq!(art.len(), 23 + 4176);
}

#[test]
fn fixture_second_track_without_artwork() {
    let t = parse_adapter_line(fixture_line(4)).unwrap().unwrap();
    assert_eq!(t.title, "Why your build is slow");
    assert_eq!(t.artist.as_deref(), Some("Compile Time"));
    assert_eq!(t.album.as_deref(), Some("Fixture Album"));
    assert_eq!(t.bundle_id, "dev.lem.notchhud.fixture");
    assert!(t.playing);
    assert_eq!(t.duration_ms, Some(3_730_000));
    assert_eq!(t.elapsed_ms, Some(61_000));
    assert_eq!(t.elapsed_at_ms, Some(1_792_597_410_778));
    assert_eq!(t.artwork, None);
}

#[test]
fn fixture_second_track_with_artwork() {
    let t = parse_adapter_line(fixture_line(5)).unwrap().unwrap();
    assert_eq!(t.title, "Why your build is slow");
    assert_eq!(t.duration_ms, Some(3_730_000));
    assert_eq!(t.elapsed_at_ms, Some(1_792_597_410_778));
    let art = t.artwork.expect("artwork");
    assert!(art.starts_with("data:image/jpeg;base64,/9j/4AAQSkZJRgABAQAASABI"));
    assert_eq!(art.len(), 23 + 3852);
}

// ---- parse_adapter_line: errors and edges ----

#[test]
fn adapter_invalid_json_is_err() {
    assert!(parse_adapter_line("not json").is_err());
    assert!(parse_adapter_line("").is_err());
}

#[test]
fn adapter_non_data_type_is_err() {
    let line = r#"{"type":"error","diff":false,"payload":{"title":"X"}}"#;
    assert!(parse_adapter_line(line).is_err());
}

#[test]
fn adapter_diff_true_is_err() {
    let line = r#"{"type":"data","diff":true,"payload":{"title":"X"}}"#;
    assert!(parse_adapter_line(line).is_err());
}

#[test]
fn adapter_missing_title_is_none() {
    let line = adapter(r#"{"artist":"A","playing":true}"#);
    assert_eq!(parse_adapter_line(&line), Ok(None));
}

#[test]
fn adapter_empty_title_is_none() {
    let line = adapter(r#"{"title":"","artist":"A","playing":true}"#);
    assert_eq!(parse_adapter_line(&line), Ok(None));
}

#[test]
fn adapter_minimal_payload_defaults() {
    let line = adapter(r#"{"title":"Solo","bundleIdentifier":"com.spotify.client"}"#);
    let t = parse_adapter_line(&line).unwrap().unwrap();
    assert_eq!(
        t,
        Track {
            title: "Solo".into(),
            artist: None,
            album: None,
            app: "Spotify".into(),
            bundle_id: "com.spotify.client".into(),
            playing: false,
            duration_ms: None,
            elapsed_ms: None,
            elapsed_at_ms: None,
            artwork: None,
        }
    );
}

#[test]
fn adapter_micros_convert_to_ms_by_truncation() {
    let line = adapter(
        r#"{"title":"T","bundleIdentifier":"b","playing":false,"durationMicros":1999999,"elapsedTimeMicros":2500999,"timestampEpochMicros":3000001}"#,
    );
    let t = parse_adapter_line(&line).unwrap().unwrap();
    assert!(!t.playing);
    assert_eq!(t.duration_ms, Some(1999));
    assert_eq!(t.elapsed_ms, Some(2500));
    assert_eq!(t.elapsed_at_ms, Some(3000));
}

#[test]
fn adapter_artwork_needs_both_mime_and_data() {
    let only_data = adapter(r#"{"title":"T","bundleIdentifier":"b","artworkData":"QUJD"}"#);
    let only_mime = adapter(r#"{"title":"T","bundleIdentifier":"b","artworkMimeType":"image/png"}"#);
    let empty_data =
        adapter(r#"{"title":"T","bundleIdentifier":"b","artworkMimeType":"image/png","artworkData":""}"#);
    let empty_mime =
        adapter(r#"{"title":"T","bundleIdentifier":"b","artworkMimeType":"","artworkData":"QUJD"}"#);
    let both = adapter(
        r#"{"title":"T","bundleIdentifier":"b","artworkMimeType":"image/png","artworkData":"QUJD"}"#,
    );
    for line in [&only_data, &only_mime, &empty_data, &empty_mime] {
        assert_eq!(parse_adapter_line(line).unwrap().unwrap().artwork, None);
    }
    assert_eq!(
        parse_adapter_line(&both).unwrap().unwrap().artwork.as_deref(),
        Some("data:image/png;base64,QUJD")
    );
}

#[test]
fn adapter_app_prefers_parent_bundle() {
    let line = adapter(
        r#"{"title":"Clip","bundleIdentifier":"com.apple.WebKit.GPU","parentApplicationBundleIdentifier":"com.apple.Safari","playing":true}"#,
    );
    let t = parse_adapter_line(&line).unwrap().unwrap();
    assert_eq!(t.app, "Safari");
    assert_eq!(t.bundle_id, "com.apple.WebKit.GPU");
}

// ---- app_name ----

#[test]
fn app_name_table() {
    let table = [
        ("com.apple.Music", "Apple Music"),
        ("com.spotify.client", "Spotify"),
        ("com.google.Chrome", "Chrome"),
        ("com.apple.Safari", "Safari"),
        ("company.thebrowser.Browser", "Arc"),
        ("org.mozilla.firefox", "Firefox"),
        ("com.microsoft.edgemac", "Edge"),
        ("com.brave.Browser", "Brave"),
        ("com.apple.podcasts", "Podcasts"),
        ("com.apple.QuickTimePlayerX", "QuickTime Player"),
    ];
    for (id, name) in table {
        assert_eq!(app_name(id, None), name, "{id}");
    }
}

#[test]
fn app_name_parent_wins_over_bundle() {
    assert_eq!(app_name("com.spotify.client", Some("com.google.Chrome")), "Chrome");
    assert_eq!(app_name("com.google.Chrome.helper", Some("com.brave.Browser")), "Brave");
}

#[test]
fn app_name_unknown_returns_looked_up_identifier() {
    assert_eq!(app_name("dev.lem.notchhud.fixture", None), "dev.lem.notchhud.fixture");
    assert_eq!(app_name("com.apple.Music", Some("org.example.unknown")), "org.example.unknown");
}

// ---- parse_applescript ----

const ART: &str = "https://i.scdn.co/image/abc123";

#[test]
fn applescript_spotify_full_line() {
    let out = format!("spotify\tplaying\tSong\tArtist\tAlbum\t200400\t12600\t{ART}\n");
    let t = parse_applescript(&out, 1_700_000_000_000).unwrap();
    assert_eq!(
        t,
        Track {
            title: "Song".into(),
            artist: Some("Artist".into()),
            album: Some("Album".into()),
            app: "Spotify".into(),
            bundle_id: "com.spotify.client".into(),
            playing: true,
            duration_ms: Some(200_400),
            elapsed_ms: Some(12_600),
            elapsed_at_ms: Some(1_700_000_000_000),
            artwork: Some(ART.into()),
        }
    );
}

#[test]
fn applescript_music_maps_to_apple_music() {
    let out = "music\tplaying\tTune\tBand\tLP\t180000\t30000\t\n";
    let t = parse_applescript(out, 5).unwrap();
    assert_eq!(t.app, "Apple Music");
    assert_eq!(t.bundle_id, "com.apple.Music");
    assert_eq!(t.duration_ms, Some(180_000));
    assert_eq!(t.elapsed_ms, Some(30_000));
    assert_eq!(t.elapsed_at_ms, Some(5));
    assert_eq!(t.artwork, None);
}

#[test]
fn applescript_rounds_fractional_ms() {
    let out = "music\tplaying\tT\tA\tB\t1234.6\t0.6\t\n";
    let t = parse_applescript(out, 0).unwrap();
    assert_eq!(t.duration_ms, Some(1235));
    assert_eq!(t.elapsed_ms, Some(1));
}

#[test]
fn applescript_empty_fields_become_none() {
    let out = "music\tplaying\tT\t\t\t10000\t1000\t\n";
    let t = parse_applescript(out, 0).unwrap();
    assert_eq!(t.artist, None);
    assert_eq!(t.album, None);
    assert_eq!(t.artwork, None);
}

#[test]
fn applescript_playing_beats_paused_regardless_of_order() {
    let out = "music\tpaused\tOld\tA\tB\t10000\t1000\t\nspotify\tplaying\tNew\tA\tB\t10000\t1000\t\n";
    let t = parse_applescript(out, 0).unwrap();
    assert_eq!(t.title, "New");
    assert!(t.playing);
}

#[test]
fn applescript_first_playing_wins() {
    let out = "music\tplaying\tFirst\tA\tB\t10000\t1000\t\nspotify\tplaying\tSecond\tA\tB\t10000\t1000\t\n";
    assert_eq!(parse_applescript(out, 0).unwrap().title, "First");
}

#[test]
fn applescript_paused_when_nothing_playing() {
    let out = "music\tstopped\tGone\tA\tB\t10000\t0\t\nspotify\tpaused\tHeld\tA\tB\t10000\t4000\t\n";
    let t = parse_applescript(out, 0).unwrap();
    assert_eq!(t.title, "Held");
    assert!(!t.playing);
    assert_eq!(t.elapsed_ms, Some(4_000));
}

#[test]
fn applescript_stopped_only_is_none() {
    assert_eq!(parse_applescript("music\tstopped\tT\tA\tB\t10000\t0\t\n", 0), None);
}

#[test]
fn applescript_skips_bad_lines() {
    let out = [
        "music\tplaying\tshort\tA\tB\t10000\t1000",           // 7 fields
        "vlc\tplaying\tUnknownApp\tA\tB\t10000\t1000\t",       // unknown app
        "music\tplaying\t\tA\tB\t10000\t1000\t",               // empty title
        "music\tplaying\tBadNum\tA\tB\tabc\t1\t",        // unparsable duration
        "music\tplaying\tBadPos\tA\tB\t10000\txyz\t",       // unparsable position
        "spotify\tplaying\tGood\tA\tB\t10000\t1000\t",
    ]
    .join("\n");
    assert_eq!(parse_applescript(&out, 0).unwrap().title, "Good");
}

#[test]
fn applescript_only_bad_lines_is_none() {
    assert_eq!(parse_applescript("garbage\n\nmusic\tplaying\t\tA\tB\t1000\t1000\t\n", 0), None);
}

#[test]
fn applescript_empty_output_is_none() {
    assert_eq!(parse_applescript("", 0), None);
}

// ---- applescript_denied ----

#[test]
fn applescript_denied_detects_1743() {
    assert!(applescript_denied(
        "execution error: Not authorized to send Apple events to Music. (-1743)"
    ));
    assert!(applescript_denied("-1743"));
}

#[test]
fn applescript_denied_false_otherwise() {
    assert!(!applescript_denied(""));
    assert!(!applescript_denied("1743"));
    assert!(!applescript_denied("execution error: Application isn't running. (-600)"));
}

// ---- backoff ----

#[test]
fn backoff_doubles_then_caps() {
    assert_eq!(backoff(0), Duration::from_secs(1));
    assert_eq!(backoff(1), Duration::from_secs(2));
    assert_eq!(backoff(5), Duration::from_secs(32));
    assert_eq!(backoff(6), Duration::from_secs(60));
    assert_eq!(backoff(30), Duration::from_secs(60));
}

#[test]
fn backoff_huge_attempt_does_not_overflow() {
    assert_eq!(backoff(u32::MAX), Duration::from_secs(60));
}

// ---- serde ----

#[test]
fn media_serializes_with_lowercase_enums_and_contract_fields() {
    let m = Media {
        via: Via::Applescript,
        health: Health::Denied,
        track: None,
    };
    assert_eq!(
        serde_json::to_string(&m).unwrap(),
        r#"{"via":"applescript","health":"denied","track":null}"#
    );
}

#[test]
fn media_with_track_serializes_all_fields_in_order() {
    let m = Media {
        via: Via::Adapter,
        health: Health::Ok,
        track: Some(Track {
            title: "Song".into(),
            artist: Some("Artist".into()),
            album: None,
            app: "Spotify".into(),
            bundle_id: "com.spotify.client".into(),
            playing: true,
            duration_ms: Some(200_000),
            elapsed_ms: Some(1_000),
            elapsed_at_ms: Some(1_700_000_000_000),
            artwork: None,
        }),
    };
    assert_eq!(
        serde_json::to_string(&m).unwrap(),
        concat!(
            r#"{"via":"adapter","health":"ok","track":{"title":"Song","artist":"Artist","album":null,"#,
            r#""app":"Spotify","bundle_id":"com.spotify.client","playing":true,"duration_ms":200000,"#,
            r#""elapsed_ms":1000,"elapsed_at_ms":1700000000000,"artwork":null}}"#
        )
    );
}

#[test]
fn via_none_and_health_failed_serialize_lowercase() {
    let m = Media {
        via: Via::None,
        health: Health::Failed,
        track: None,
    };
    assert_eq!(
        serde_json::to_string(&m).unwrap(),
        r#"{"via":"none","health":"failed","track":null}"#
    );
}

// ---- controls: parse_action ----

use serde_json::json;

fn v(s: &str) -> serde_json::Value {
    serde_json::from_str(s).expect("test json")
}

fn media_with(via: Via, bundle_id: &str, duration_ms: Option<u64>) -> Media {
    Media {
        via,
        health: Health::Ok,
        track: Some(Track {
            title: "Song".into(),
            artist: Some("Artist".into()),
            album: None,
            app: "App".into(),
            bundle_id: bundle_id.into(),
            playing: true,
            duration_ms,
            elapsed_ms: Some(1_000),
            elapsed_at_ms: Some(1_700_000_000_000),
            artwork: None,
        }),
    }
}

fn script(app: &'static str, script: &str) -> Route {
    Route::Script {
        app,
        script: script.into(),
    }
}

fn args(a: &[&str]) -> Route {
    Route::Adapter(a.iter().map(|s| s.to_string()).collect())
}

#[test]
fn parse_action_accepts_the_four_shapes() {
    assert_eq!(parse_action(&v(r#"{"kind":"toggle"}"#)), Ok(Action::Toggle));
    assert_eq!(parse_action(&v(r#"{"kind":"next"}"#)), Ok(Action::Next));
    assert_eq!(parse_action(&v(r#"{"kind":"previous"}"#)), Ok(Action::Previous));
    assert_eq!(
        parse_action(&v(r#"{"kind":"seek","position_ms":74000}"#)),
        Ok(Action::Seek { position_ms: 74_000 })
    );
}

#[test]
fn parse_action_accepts_seek_zero_and_the_maximum() {
    assert_eq!(
        parse_action(&json!({"kind": "seek", "position_ms": 0})),
        Ok(Action::Seek { position_ms: 0 })
    );
    assert_eq!(MAX_SEEK_MS, 86_400_000);
    assert_eq!(
        parse_action(&json!({"kind": "seek", "position_ms": 86_400_000u64})),
        Ok(Action::Seek { position_ms: 86_400_000 })
    );
}

#[test]
fn parse_action_rejects_one_past_the_maximum() {
    assert!(parse_action(&json!({"kind": "seek", "position_ms": 86_400_001u64})).is_err());
}

#[test]
fn parse_action_rejects_non_objects() {
    for raw in ["null", "true", "5", r#""toggle""#, "[]", r#"[{"kind":"toggle"}]"#] {
        assert!(parse_action(&v(raw)).is_err(), "{raw}");
    }
}

#[test]
fn parse_action_rejects_missing_or_unknown_kind() {
    for raw in [
        "{}",
        r#"{"position_ms":5}"#,
        r#"{"kind":"play"}"#,
        r#"{"kind":"pause"}"#,
        r#"{"kind":"Toggle"}"#,
        r#"{"kind":"TOGGLE"}"#,
        r#"{"kind":""}"#,
        r#"{"kind":null}"#,
        r#"{"kind":5}"#,
        r#"{"kind":"stop"}"#,
    ] {
        assert!(parse_action(&v(raw)).is_err(), "{raw}");
    }
}

#[test]
fn parse_action_rejects_extra_fields() {
    for raw in [
        r#"{"kind":"toggle","position_ms":1}"#,
        r#"{"kind":"next","x":1}"#,
        r#"{"kind":"previous","extra":true}"#,
        r#"{"kind":"seek","position_ms":1,"extra":1}"#,
    ] {
        assert!(parse_action(&v(raw)).is_err(), "{raw}");
    }
}

#[test]
fn parse_action_rejects_bad_seek_positions() {
    for raw in [
        r#"{"kind":"seek"}"#,
        r#"{"kind":"seek","position_ms":-1}"#,
        r#"{"kind":"seek","position_ms":1.5}"#,
        r#"{"kind":"seek","position_ms":"74000"}"#,
        r#"{"kind":"seek","position_ms":null}"#,
        r#"{"kind":"seek","position_ms":true}"#,
        r#"{"kind":"seek","position_ms":1e12}"#,
    ] {
        assert!(parse_action(&v(raw)).is_err(), "{raw}");
    }
}

// ---- controls: route ----

#[test]
fn route_adapter_sends_the_media_key_codes() {
    let m = media_with(Via::Adapter, "com.apple.Music", Some(223_000));
    assert_eq!(route(Some(&m), Action::Toggle), Ok(args(&["send", "2"])));
    assert_eq!(route(Some(&m), Action::Next), Ok(args(&["send", "4"])));
    assert_eq!(route(Some(&m), Action::Previous), Ok(args(&["send", "5"])));
}

#[test]
fn route_adapter_seek_is_in_microseconds() {
    let m = media_with(Via::Adapter, "com.apple.Music", Some(223_000));
    assert_eq!(
        route(Some(&m), Action::Seek { position_ms: 74_000 }),
        Ok(args(&["seek", "74000000"]))
    );
    assert_eq!(
        route(Some(&m), Action::Seek { position_ms: 0 }),
        Ok(args(&["seek", "0"]))
    );
}

#[test]
fn route_seek_clamps_to_the_duration() {
    let m = media_with(Via::Adapter, "com.apple.Music", Some(223_000));
    assert_eq!(
        route(Some(&m), Action::Seek { position_ms: 300_000 }),
        Ok(args(&["seek", "223000000"]))
    );
    let s = media_with(Via::Applescript, "com.apple.Music", Some(223_000));
    assert_eq!(
        route(Some(&s), Action::Seek { position_ms: 300_000 }),
        Ok(script("Music", r#"tell application "Music" to set player position to 223.000"#))
    );
}

#[test]
fn route_seek_does_not_clamp_without_a_duration() {
    let m = media_with(Via::Adapter, "com.apple.Music", None);
    assert_eq!(
        route(Some(&m), Action::Seek { position_ms: 300_000 }),
        Ok(args(&["seek", "300000000"]))
    );
}

#[test]
fn route_applescript_music_scripts() {
    let m = media_with(Via::Applescript, "com.apple.Music", Some(223_000));
    assert_eq!(
        route(Some(&m), Action::Toggle),
        Ok(script("Music", r#"tell application "Music" to playpause"#))
    );
    assert_eq!(
        route(Some(&m), Action::Next),
        Ok(script("Music", r#"tell application "Music" to next track"#))
    );
    assert_eq!(
        route(Some(&m), Action::Previous),
        Ok(script("Music", r#"tell application "Music" to previous track"#))
    );
    assert_eq!(
        route(Some(&m), Action::Seek { position_ms: 74_000 }),
        Ok(script("Music", r#"tell application "Music" to set player position to 74.000"#))
    );
}

#[test]
fn route_applescript_spotify_scripts() {
    let m = media_with(Via::Applescript, "com.spotify.client", Some(200_000));
    assert_eq!(
        route(Some(&m), Action::Toggle),
        Ok(script("Spotify", r#"tell application "Spotify" to playpause"#))
    );
    assert_eq!(
        route(Some(&m), Action::Next),
        Ok(script("Spotify", r#"tell application "Spotify" to next track"#))
    );
    assert_eq!(
        route(Some(&m), Action::Previous),
        Ok(script("Spotify", r#"tell application "Spotify" to previous track"#))
    );
    assert_eq!(
        route(Some(&m), Action::Seek { position_ms: 74_000 }),
        Ok(script("Spotify", r#"tell application "Spotify" to set player position to 74.000"#))
    );
}

#[test]
fn route_applescript_seek_pads_milliseconds_to_three_digits() {
    let m = media_with(Via::Applescript, "com.apple.Music", Some(223_000));
    assert_eq!(
        route(Some(&m), Action::Seek { position_ms: 5 }),
        Ok(script("Music", r#"tell application "Music" to set player position to 0.005"#))
    );
    assert_eq!(
        route(Some(&m), Action::Seek { position_ms: 61_050 }),
        Ok(script("Music", r#"tell application "Music" to set player position to 61.050"#))
    );
}

#[test]
fn route_applescript_unknown_bundle_id_is_an_error() {
    let m = media_with(Via::Applescript, "com.google.Chrome", Some(223_000));
    for a in [Action::Toggle, Action::Next, Action::Previous, Action::Seek { position_ms: 1 }] {
        assert!(route(Some(&m), a).is_err(), "{a:?}");
    }
}

#[test]
fn route_via_none_is_an_error() {
    let m = media_with(Via::None, "com.apple.Music", Some(223_000));
    for a in [Action::Toggle, Action::Next, Action::Previous, Action::Seek { position_ms: 1 }] {
        assert!(route(Some(&m), a).is_err(), "{a:?}");
    }
}

#[test]
fn route_without_media_or_track_is_nothing_playing() {
    assert_eq!(route(None, Action::Toggle), Err("nothing playing".to_string()));
    let m = Media {
        via: Via::Adapter,
        health: Health::Ok,
        track: None,
    };
    assert_eq!(route(Some(&m), Action::Next), Err("nothing playing".to_string()));
    assert_eq!(
        route(Some(&m), Action::Seek { position_ms: 5 }),
        Err("nothing playing".to_string())
    );
}

// ---- controls: apply ----

#[test]
fn apply_script_for_a_player_that_is_not_running_is_an_error() {
    let r = script("NoSuchPlayerXyz", r#"tell application "NoSuchPlayerXyz" to playpause"#);
    let err = apply(&r).expect_err("must not run");
    assert!(err.contains("not running"), "{err}");
}

// ---- adapter_dir_for ----

use std::ffi::OsStr;
use std::path::{Path, PathBuf};

const BUNDLE_EXE: &str = "/Applications/notch-hud.app/Contents/MacOS/notch-hud";

#[test]
fn adapter_dir_env_wins_over_a_bundle_exe() {
    assert_eq!(
        adapter_dir_for(Some(OsStr::new("/custom/adapter")), Some(Path::new(BUNDLE_EXE))),
        PathBuf::from("/custom/adapter")
    );
}

#[test]
fn adapter_dir_empty_env_is_ignored() {
    assert_eq!(
        adapter_dir_for(Some(OsStr::new("")), Some(Path::new(BUNDLE_EXE))),
        PathBuf::from("/Applications/notch-hud.app/Contents/Resources/adapter")
    );
}

#[test]
fn adapter_dir_bundle_exe_uses_contents_resources() {
    assert_eq!(
        adapter_dir_for(None, Some(Path::new(BUNDLE_EXE))),
        PathBuf::from("/Applications/notch-hud.app/Contents/Resources/adapter")
    );
}

#[test]
fn adapter_dir_dev_release_exe_uses_target_adapter() {
    assert_eq!(
        adapter_dir_for(None, Some(Path::new("/r/src-tauri/target/release/notch-hud"))),
        PathBuf::from("/r/src-tauri/target/adapter")
    );
}

#[test]
fn adapter_dir_dev_debug_exe_uses_target_adapter() {
    assert_eq!(
        adapter_dir_for(None, Some(Path::new("/r/src-tauri/target/debug/notch-hud"))),
        PathBuf::from("/r/src-tauri/target/adapter")
    );
}

#[test]
fn adapter_dir_macos_dir_not_under_contents_is_not_a_bundle() {
    assert_eq!(
        adapter_dir_for(None, Some(Path::new("/x/MacOS/notch-hud"))),
        PathBuf::from("/x/adapter")
    );
}

#[test]
fn adapter_dir_without_exe_is_relative() {
    assert_eq!(adapter_dir_for(None, None), PathBuf::from("adapter"));
}

#[test]
fn adapter_dir_exe_without_grandparent_is_relative() {
    assert_eq!(adapter_dir_for(None, Some(Path::new("notch-hud"))), PathBuf::from("adapter"));
}
