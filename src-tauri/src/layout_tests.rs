// Tests for CONTRACT.md section 8, "Geometry". Expected values are written out from the contract's formulas for a
// 185 x 32 notch on a 982 pt tall screen; none are computed with the code under test.
use super::*;

fn size(width: f64, height: f64) -> Size {
    Size { width, height }
}

/// The notch used throughout: 185 x 32 on a 982 pt tall screen.
fn sample(rows: usize, media_extra: f64) -> Layout {
    layout(185.0, 32.0, 982.0, rows, media_extra)
}

#[test]
fn layout_with_a_track_and_one_row() {
    assert_eq!(
        sample(1, 146.0),
        Layout {
            stage: size(452.0, 982.0),
            pill: size(305.0, 32.0),
            peek: size(309.0, 36.0),
            banner: size(380.0, 78.0),
            open: size(420.0, 226.0),
            ear: 60.0,
        }
    );
}

#[test]
fn zero_rows_count_as_one() {
    assert_eq!(sample(0, 146.0), sample(1, 146.0));
    assert_eq!(sample(0, 146.0).open, size(420.0, 226.0));
    assert_eq!(sample(0, 0.0).open, size(420.0, 80.0));
}

#[test]
fn open_height_follows_rows_and_media_extra() {
    assert_eq!(sample(1, 22.0).open, size(420.0, 102.0));
    assert_eq!(sample(1, 0.0).open, size(420.0, 80.0));
    assert_eq!(sample(3, 0.0).open, size(420.0, 124.0));
    assert_eq!(sample(3, 146.0).open, size(420.0, 270.0));
    assert_eq!(sample(10, 146.0).open, size(420.0, 424.0));
}

#[test]
fn only_open_depends_on_rows_and_media_extra() {
    let a = sample(1, 146.0);
    let b = sample(7, 22.0);
    assert_eq!((a.stage, a.pill, a.peek, a.banner, a.ear), (b.stage, b.pill, b.peek, b.banner, b.ear));
}

#[test]
fn pill_and_banner_follow_the_notch_and_stage_follows_the_screen() {
    let l = layout(200.0, 38.0, 1117.0, 1, 146.0);
    assert_eq!(l.pill, size(320.0, 38.0));
    assert_eq!(l.peek, size(324.0, 42.0));
    assert_eq!(l.banner, size(380.0, 84.0));
    assert_eq!(l.stage, size(452.0, 1117.0));
    assert_eq!(l.open, size(420.0, 226.0));
    assert_eq!(l.ear, 60.0);
}

#[test]
fn constants() {
    assert_eq!(EAR, 60.0);
    assert_eq!(EXPANDED_WIDTH, 420.0);
    assert_eq!(BANNER_WIDTH, 380.0);
    assert_eq!(MARGIN, 16.0);
}

#[test]
fn shape_size_returns_the_matching_field() {
    let l = sample(1, 146.0);
    assert_eq!(shape_size(&l, Shape::Pill), size(305.0, 32.0));
    assert_eq!(shape_size(&l, Shape::Peek), size(309.0, 36.0));
    assert_eq!(shape_size(&l, Shape::Banner), size(380.0, 78.0));
    assert_eq!(shape_size(&l, Shape::Open), size(420.0, 226.0));
}

#[test]
fn window_size_per_mode() {
    let l = sample(1, 146.0);
    assert_eq!(window_size(&l, WindowMode::Pill), size(305.0, 32.0));
    assert_eq!(window_size(&l, WindowMode::Banner), size(412.0, 94.0));
    assert_eq!(window_size(&l, WindowMode::Open), size(452.0, 242.0));
}

#[test]
fn window_size_follows_the_layout() {
    assert_eq!(window_size(&sample(1, 22.0), WindowMode::Open), size(452.0, 118.0));
    assert_eq!(window_size(&sample(0, 0.0), WindowMode::Open), size(452.0, 96.0));
    assert_eq!(window_size(&sample(4, 146.0), WindowMode::Open), size(452.0, 308.0));
    let tall = layout(200.0, 38.0, 1117.0, 1, 146.0);
    assert_eq!(window_size(&tall, WindowMode::Pill), size(320.0, 38.0));
    assert_eq!(window_size(&tall, WindowMode::Banner), size(412.0, 100.0));
}

#[test]
fn the_open_window_is_never_narrower_than_the_stage() {
    let l = sample(1, 146.0);
    assert_eq!(window_size(&l, WindowMode::Open).width, l.stage.width);
}

#[test]
fn shape_and_window_mode_parse_lowercase_names() {
    let shape = |s: &str| serde_json::from_str::<Shape>(s);
    assert_eq!(shape("\"pill\"").unwrap(), Shape::Pill);
    assert_eq!(shape("\"peek\"").unwrap(), Shape::Peek);
    assert_eq!(shape("\"banner\"").unwrap(), Shape::Banner);
    assert_eq!(shape("\"open\"").unwrap(), Shape::Open);
    let mode = |s: &str| serde_json::from_str::<WindowMode>(s);
    assert_eq!(mode("\"pill\"").unwrap(), WindowMode::Pill);
    assert_eq!(mode("\"banner\"").unwrap(), WindowMode::Banner);
    assert_eq!(mode("\"open\"").unwrap(), WindowMode::Open);
}

#[test]
fn shape_and_window_mode_reject_everything_else() {
    for bad in ["\"Pill\"", "\"OPEN\"", "\"expanded\"", "\"\"", "\" pill\"", "\"pill \"", "0", "null", "true", "[]", "{}"] {
        assert!(serde_json::from_str::<Shape>(bad).is_err(), "Shape accepted {bad}");
        assert!(serde_json::from_str::<WindowMode>(bad).is_err(), "WindowMode accepted {bad}");
    }
    // Peek is a shape (a hit rect) but never a window mode.
    assert!(serde_json::from_str::<WindowMode>("\"peek\"").is_err());
}

#[test]
fn layout_serializes_with_the_contract_field_names() {
    let value = serde_json::to_value(sample(1, 146.0)).unwrap();
    assert_eq!(
        value,
        serde_json::json!({
            "stage": { "width": 452.0, "height": 982.0 },
            "pill": { "width": 305.0, "height": 32.0 },
            "peek": { "width": 309.0, "height": 36.0 },
            "banner": { "width": 380.0, "height": 78.0 },
            "open": { "width": 420.0, "height": 226.0 },
            "ear": 60.0,
        })
    );
}
