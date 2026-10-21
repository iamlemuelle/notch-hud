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
    let out = format!("spotify\tplaying\tSong\tArtist\tAlbum\t200.4\t12.6\t{ART}\n");
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
    let out = "music\tplaying\tTune\tBand\tLP\t180\t30\t\n";
    let t = parse_applescript(out, 5).unwrap();
    assert_eq!(t.app, "Apple Music");
    assert_eq!(t.bundle_id, "com.apple.Music");
    assert_eq!(t.duration_ms, Some(180_000));
    assert_eq!(t.elapsed_ms, Some(30_000));
    assert_eq!(t.elapsed_at_ms, Some(5));
    assert_eq!(t.artwork, None);
}

#[test]
fn applescript_rounds_seconds_to_ms() {
    let out = "music\tplaying\tT\tA\tB\t1.2346\t0.0006\t\n";
    let t = parse_applescript(out, 0).unwrap();
    assert_eq!(t.duration_ms, Some(1235));
    assert_eq!(t.elapsed_ms, Some(1));
}

#[test]
fn applescript_empty_fields_become_none() {
    let out = "music\tplaying\tT\t\t\t10\t1\t\n";
    let t = parse_applescript(out, 0).unwrap();
    assert_eq!(t.artist, None);
    assert_eq!(t.album, None);
    assert_eq!(t.artwork, None);
}

#[test]
fn applescript_playing_beats_paused_regardless_of_order() {
    let out = "music\tpaused\tOld\tA\tB\t10\t1\t\nspotify\tplaying\tNew\tA\tB\t10\t1\t\n";
    let t = parse_applescript(out, 0).unwrap();
    assert_eq!(t.title, "New");
    assert!(t.playing);
}

#[test]
fn applescript_first_playing_wins() {
    let out = "music\tplaying\tFirst\tA\tB\t10\t1\t\nspotify\tplaying\tSecond\tA\tB\t10\t1\t\n";
    assert_eq!(parse_applescript(out, 0).unwrap().title, "First");
}

#[test]
fn applescript_paused_when_nothing_playing() {
    let out = "music\tstopped\tGone\tA\tB\t10\t0\t\nspotify\tpaused\tHeld\tA\tB\t10\t4\t\n";
    let t = parse_applescript(out, 0).unwrap();
    assert_eq!(t.title, "Held");
    assert!(!t.playing);
    assert_eq!(t.elapsed_ms, Some(4_000));
}

#[test]
fn applescript_stopped_only_is_none() {
    assert_eq!(parse_applescript("music\tstopped\tT\tA\tB\t10\t0\t\n", 0), None);
}

#[test]
fn applescript_skips_bad_lines() {
    let out = [
        "music\tplaying\tshort\tA\tB\t10\t1",           // 7 fields
        "vlc\tplaying\tUnknownApp\tA\tB\t10\t1\t",       // unknown app
        "music\tplaying\t\tA\tB\t10\t1\t",               // empty title
        "music\tplaying\tBadNum\tA\tB\tabc\t1\t",        // unparsable duration
        "music\tplaying\tBadPos\tA\tB\t10\txyz\t",       // unparsable position
        "spotify\tplaying\tGood\tA\tB\t10\t1\t",
    ]
    .join("\n");
    assert_eq!(parse_applescript(&out, 0).unwrap().title, "Good");
}

#[test]
fn applescript_only_bad_lines_is_none() {
    assert_eq!(parse_applescript("garbage\n\nmusic\tplaying\t\tA\tB\t1\t1\t\n", 0), None);
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
