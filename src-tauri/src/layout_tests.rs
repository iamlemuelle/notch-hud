// Tests for CONTRACT.md section 8, "Geometry". Expected values are written out from the contract's formulas for a
// 185 x 32 notch on a 1512 x 982 pt screen; none are computed with the code under test.
use super::*;

fn size(width: f64, height: f64) -> Size {
    Size { width, height }
}

/// The notch used throughout: 185 x 32 on a 1512 x 982 pt screen.
fn sample(rows: usize, total: usize) -> Layout {
    layout(185.0, 32.0, 1512.0, 982.0, rows, total)
}

#[test]
fn layout_with_one_row() {
    assert_eq!(
        sample(1, 1),
        Layout {
            stage: size(640.0, 982.0),
            theater: size(608.0, 420.0),
            pill: size(305.0, 32.0),
            peek: size(309.0, 36.0),
            banner: size(380.0, 78.0),
            open: size(420.0, 254.0),
            detail: size(420.0, 380.0),
            ear: 60.0,
        }
    );
}

#[test]
fn zero_rows_count_as_one() {
    assert_eq!(sample(0, 0), sample(1, 1));
    assert_eq!(sample(0, 0).open, size(420.0, 254.0));
}

#[test]
fn open_height_follows_rows_and_total() {
    assert_eq!(sample(1, 1).open, size(420.0, 254.0));
    assert_eq!(sample(3, 3).open, size(420.0, 298.0));
    assert_eq!(sample(4, 4).open, size(420.0, 320.0));
    assert_eq!(sample(4, 5).open, size(420.0, 342.0));
    assert_eq!(sample(4, 50).open, size(420.0, 342.0));
}

#[test]
fn only_open_depends_on_rows_and_total() {
    let a = sample(1, 1);
    let b = sample(4, 9);
    assert_eq!(
        (a.stage, a.pill, a.peek, a.banner, a.detail, a.ear),
        (b.stage, b.pill, b.peek, b.banner, b.detail, b.ear)
    );
}

#[test]
fn pill_and_banner_follow_the_notch_and_stage_follows_the_screen() {
    let l = layout(200.0, 38.0, 1512.0, 1117.0, 1, 1);
    assert_eq!(l.pill, size(320.0, 38.0));
    assert_eq!(l.peek, size(324.0, 42.0));
    assert_eq!(l.banner, size(380.0, 84.0));
    assert_eq!(l.stage, size(640.0, 1117.0));
    assert_eq!(l.theater, size(608.0, 426.0));
    assert_eq!(l.open, size(420.0, 254.0));
    assert_eq!(l.detail, size(420.0, 380.0));
    assert_eq!(l.ear, 60.0);
}

#[test]
fn constants() {
    assert_eq!(EAR, 60.0);
    assert_eq!(EXPANDED_WIDTH, 420.0);
    assert_eq!(THEATER_WIDTH, 608.0);
    assert_eq!(BANNER_WIDTH, 380.0);
    assert_eq!(MARGIN, 16.0);
    assert_eq!(DETAIL_HEIGHT, 380.0);
}

#[test]
fn detail_is_420_by_380_whatever_the_rows_total_and_notch() {
    assert_eq!(sample(1, 1).detail, size(420.0, 380.0));
    assert_eq!(sample(0, 0).detail, size(420.0, 380.0));
    assert_eq!(sample(4, 4).detail, size(420.0, 380.0));
    assert_eq!(sample(4, 50).detail, size(420.0, 380.0));
    assert_eq!(layout(200.0, 38.0, 1512.0, 1117.0, 3, 3).detail, size(420.0, 380.0));
    assert_eq!(layout(150.0, 30.0, 1512.0, 800.0, 2, 9).detail, size(420.0, 380.0));
}

#[test]
fn shape_size_returns_the_matching_field() {
    let l = sample(1, 1);
    assert_eq!(shape_size(&l, Shape::Pill), size(305.0, 32.0));
    assert_eq!(shape_size(&l, Shape::Peek), size(309.0, 36.0));
    assert_eq!(shape_size(&l, Shape::Banner), size(380.0, 78.0));
    assert_eq!(shape_size(&l, Shape::Open), size(420.0, 254.0));
    assert_eq!(shape_size(&l, Shape::Detail), size(420.0, 380.0));
    assert_eq!(shape_size(&l, Shape::Theater), size(608.0, 420.0));
}

#[test]
fn window_size_per_mode() {
    let l = sample(1, 1);
    assert_eq!(window_size(&l, WindowMode::Pill), size(305.0, 32.0));
    assert_eq!(window_size(&l, WindowMode::Banner), size(412.0, 94.0));
    assert_eq!(window_size(&l, WindowMode::Open), size(452.0, 396.0));
    assert_eq!(window_size(&l, WindowMode::Theater), size(640.0, 436.0));
}

#[test]
fn window_size_follows_the_layout() {
    assert_eq!(window_size(&sample(4, 5), WindowMode::Open), size(452.0, 396.0));
    assert_eq!(window_size(&sample(0, 0), WindowMode::Open), size(452.0, 396.0));
    assert_eq!(window_size(&sample(1, 1), WindowMode::Open), size(452.0, 396.0));
    assert_eq!(window_size(&sample(4, 4), WindowMode::Open), size(452.0, 396.0));
    assert_eq!(window_size(&sample(4, 50), WindowMode::Open), size(452.0, 396.0));
    let tall = layout(200.0, 38.0, 1512.0, 1117.0, 1, 1);
    assert_eq!(window_size(&tall, WindowMode::Pill), size(320.0, 38.0));
    assert_eq!(window_size(&tall, WindowMode::Banner), size(412.0, 100.0));
}

#[test]
fn the_open_window_is_open_width_plus_margins_and_the_stage_is_wider() {
    let l = sample(1, 1);
    assert_eq!(window_size(&l, WindowMode::Open).width, 452.0);
    assert_eq!(l.stage.width, 640.0);
}

#[test]
fn shape_and_window_mode_parse_lowercase_names() {
    let shape = |s: &str| serde_json::from_str::<Shape>(s);
    assert_eq!(shape("\"pill\"").unwrap(), Shape::Pill);
    assert_eq!(shape("\"peek\"").unwrap(), Shape::Peek);
    assert_eq!(shape("\"banner\"").unwrap(), Shape::Banner);
    assert_eq!(shape("\"open\"").unwrap(), Shape::Open);
    assert_eq!(shape("\"detail\"").unwrap(), Shape::Detail);
    assert_eq!(shape("\"theater\"").unwrap(), Shape::Theater);
    let mode = |s: &str| serde_json::from_str::<WindowMode>(s);
    assert_eq!(mode("\"pill\"").unwrap(), WindowMode::Pill);
    assert_eq!(mode("\"banner\"").unwrap(), WindowMode::Banner);
    assert_eq!(mode("\"open\"").unwrap(), WindowMode::Open);
    assert_eq!(mode("\"theater\"").unwrap(), WindowMode::Theater);
}

#[test]
fn shape_and_window_mode_reject_everything_else() {
    for bad in ["\"Pill\"", "\"OPEN\"", "\"expanded\"", "\"\"", "\" pill\"", "\"pill \"", "0", "null", "true", "[]", "{}"] {
        assert!(serde_json::from_str::<Shape>(bad).is_err(), "Shape accepted {bad}");
        assert!(serde_json::from_str::<WindowMode>(bad).is_err(), "WindowMode accepted {bad}");
    }
    // Peek is a shape (a hit rect) but never a window mode.
    assert!(serde_json::from_str::<WindowMode>("\"peek\"").is_err());
    // Detail is a shape (a hit rect and clip) but never a window mode.
    assert!(serde_json::from_str::<WindowMode>("\"detail\"").is_err());
}

#[test]
fn layout_serializes_with_the_contract_field_names() {
    let value = serde_json::to_value(sample(1, 1)).unwrap();
    assert_eq!(
        value,
        serde_json::json!({
            "stage": { "width": 640.0, "height": 982.0 },
            "pill": { "width": 305.0, "height": 32.0 },
            "peek": { "width": 309.0, "height": 36.0 },
            "banner": { "width": 380.0, "height": 78.0 },
            "open": { "width": 420.0, "height": 254.0 },
            "detail": { "width": 420.0, "height": 380.0 },
            "theater": { "width": 608.0, "height": 420.0 },
            "ear": 60.0,
        })
    );
}

// Surface, hit_contains and next_hover: CONTRACT.md section 8, "Geometry".

fn surface(window: WindowMode, hit: Shape) -> Surface {
    let mut s = Surface::new();
    s.set(window, hit);
    s
}

#[test]
fn a_new_surface_is_pill_and_pill() {
    let s = Surface::new();
    assert_eq!((s.window, s.hit), (WindowMode::Pill, Shape::Pill));
}

#[test]
fn set_records_both_values() {
    let s = surface(WindowMode::Banner, Shape::Banner);
    assert_eq!((s.window, s.hit), (WindowMode::Banner, Shape::Banner));
    let s = surface(WindowMode::Open, Shape::Peek);
    assert_eq!((s.window, s.hit), (WindowMode::Open, Shape::Peek));
}

#[test]
fn a_fresh_surface_is_the_pill_window() {
    assert_eq!(Surface::new().window_size(&sample(1, 1)), size(305.0, 32.0));
}

#[test]
fn an_open_surface_is_the_plain_open_window_at_first() {
    let mut s = surface(WindowMode::Open, Shape::Open);
    assert_eq!(s.window_size(&sample(1, 1)), size(452.0, 396.0));
}

/// A layout whose list is `open_height` tall (a literal, so a test can make it taller than the card): the window is
/// `open_height + 16` while that exceeds the card's 380 + 16.
fn tall(open_height: f64) -> Layout {
    Layout { open: size(420.0, open_height), ..sample(1, 1) }
}

#[test]
fn the_open_height_never_drops_while_open() {
    let mut s = surface(WindowMode::Open, Shape::Open);
    assert_eq!(s.window_size(&tall(500.0)), size(452.0, 516.0));
    // The list shrinks to the card's size: the plain open window would be 396, the floor holds it at 516.
    assert_eq!(s.window_size(&sample(1, 1)), size(452.0, 516.0));
    assert_eq!(s.window_size(&sample(0, 0)), size(452.0, 516.0));
    // Growing past the floor raises it.
    assert_eq!(s.window_size(&tall(600.0)), size(452.0, 616.0));
    assert_eq!(s.window_size(&sample(1, 1)), size(452.0, 616.0));
}

#[test]
fn the_open_window_is_396_for_every_row_count_through_a_surface() {
    let mut s = surface(WindowMode::Open, Shape::Detail);
    for (rows, total) in [(1, 1), (0, 0), (4, 4), (4, 5), (1, 1)] {
        assert_eq!(s.window_size(&sample(rows, total)), size(452.0, 396.0), "{rows} rows, total {total}");
    }
}

#[test]
fn a_set_with_the_same_mode_keeps_the_floor() {
    let mut s = surface(WindowMode::Open, Shape::Peek);
    assert_eq!(s.window_size(&tall(500.0)), size(452.0, 516.0));
    s.set(WindowMode::Open, Shape::Open);
    assert_eq!(s.window_size(&sample(1, 1)), size(452.0, 516.0));
    s.set(WindowMode::Open, Shape::Detail);
    assert_eq!(s.window_size(&sample(1, 1)), size(452.0, 516.0));
}

#[test]
fn a_set_with_another_mode_resets_the_floor() {
    for other in [WindowMode::Pill, WindowMode::Banner] {
        let mut s = surface(WindowMode::Open, Shape::Open);
        assert_eq!(s.window_size(&tall(500.0)), size(452.0, 516.0));
        s.set(other, Shape::Pill);
        s.set(WindowMode::Open, Shape::Peek);
        assert_eq!(s.window_size(&sample(1, 1)), size(452.0, 396.0), "after {other:?}");
    }
}

#[test]
fn the_floor_does_not_touch_pill_and_banner_sizes() {
    let mut s = surface(WindowMode::Open, Shape::Open);
    assert_eq!(s.window_size(&tall(500.0)), size(452.0, 516.0));
    s.set(WindowMode::Banner, Shape::Banner);
    assert_eq!(s.window_size(&sample(3, 3)), size(412.0, 94.0));
    s.set(WindowMode::Pill, Shape::Pill);
    assert_eq!(s.window_size(&sample(3, 3)), size(305.0, 32.0));
}

#[test]
fn a_list_taller_than_the_card_sets_the_window() {
    assert_eq!(window_size(&tall(500.0), WindowMode::Open), size(452.0, 516.0));
    assert_eq!(window_size(&tall(380.0), WindowMode::Open), size(452.0, 396.0));
    assert_eq!(window_size(&tall(300.0), WindowMode::Open), size(452.0, 396.0));
}

// A 305 x 32 pill centered on x = 100 with its top edge at y = 1000: x from -52.5 to 252.5, y from 968 to 1000.
#[test]
fn hit_contains_includes_every_edge() {
    let pill = size(305.0, 32.0);
    for (x, y) in [(-52.5, 984.0), (252.5, 984.0), (100.0, 1000.0), (100.0, 968.0), (-52.5, 1000.0), (252.5, 968.0), (100.0, 984.0)] {
        assert!(hit_contains(pill, 100.0, 1000.0, x, y), "({x}, {y}) should be inside");
    }
}

#[test]
fn hit_contains_excludes_one_point_past_each_edge() {
    let pill = size(305.0, 32.0);
    for (x, y) in [(-52.6, 984.0), (252.6, 984.0), (100.0, 1000.1), (100.0, 967.9)] {
        assert!(!hit_contains(pill, 100.0, 1000.0, x, y), "({x}, {y}) should be outside");
    }
}

#[test]
fn hit_contains_has_y_up() {
    let pill = size(305.0, 32.0);
    // Below the top edge is inside, above it is outside; a y-down reading would say the opposite.
    assert!(hit_contains(pill, 100.0, 1000.0, 100.0, 990.0));
    assert!(!hit_contains(pill, 100.0, 1000.0, 100.0, 1010.0));
    assert!(!hit_contains(pill, 100.0, 1000.0, 100.0, 10.0));
}

#[test]
fn hit_contains_follows_the_given_center_and_top() {
    let pill = size(305.0, 32.0);
    assert!(hit_contains(pill, 1000.0, 50.0, 1100.0, 40.0));
    assert!(!hit_contains(pill, 1000.0, 50.0, 100.0, 40.0));
    assert!(!hit_contains(pill, 1000.0, 50.0, 1100.0, 10.0));
}

#[test]
fn a_peek_hit_rect_is_wider_and_taller_than_the_pill() {
    let l = sample(1, 1);
    let (pill, peek) = (shape_size(&l, Shape::Pill), shape_size(&l, Shape::Peek));
    // x = 254 is 154 from the center: past the pill's 152.5, inside the peek's 154.5.
    assert!(!hit_contains(pill, 100.0, 1000.0, 254.0, 990.0));
    assert!(hit_contains(peek, 100.0, 1000.0, 254.0, 990.0));
    // y = 965 is 35 below the top: past the pill's 32, inside the peek's 36.
    assert!(!hit_contains(pill, 100.0, 1000.0, 100.0, 965.0));
    assert!(hit_contains(peek, 100.0, 1000.0, 100.0, 965.0));
    assert!(!hit_contains(peek, 100.0, 1000.0, 100.0, 963.9));
}

#[test]
fn next_hover_over_all_eight_inputs() {
    // (was_in, over, moved, next)
    let table = [
        (false, false, false, false),
        (false, false, true, false),
        (true, false, false, false),
        (true, false, true, false),
        (false, true, false, false),
        (false, true, true, true),
        (true, true, false, true),
        (true, true, true, true),
    ];
    for (was_in, over, moved, want) in table {
        assert_eq!(next_hover(was_in, over, moved), want, "was_in={was_in} over={over} moved={moved}");
    }
}

#[test]
fn a_still_cursor_under_a_grown_shape_stays_out() {
    let mut in_now = false;
    for _ in 0..10 {
        in_now = next_hover(in_now, true, false);
    }
    assert!(!in_now);
    in_now = next_hover(in_now, true, true);
    assert!(in_now);
    // Once in, a still cursor stays in.
    assert!(next_hover(in_now, true, false));
}

#[test]
fn leaving_needs_no_movement() {
    assert!(!next_hover(true, false, false));
}

#[test]
fn expanded_height_follows_shown_rows() {
    assert_eq!(expanded_height(0, 0), 254.0);
    assert_eq!(expanded_height(1, 1), 254.0);
    assert_eq!(expanded_height(3, 3), 298.0);
    assert_eq!(expanded_height(4, 4), 320.0);
    assert_eq!(expanded_height(4, 5), 342.0);
    assert_eq!(expanded_height(4, 50), 342.0);
}

// The video stage (CONTRACT.md section 8, "Theater"): 608 wide, 16:9 picture inset 16, notch + 388 tall; half the
// screen's width when that is narrower.

/// A 185 x 32 notch on a 1100 x 982 pt screen: half the width is 550.
fn narrow() -> Layout {
    layout(185.0, 32.0, 1100.0, 982.0, 1, 1)
}

#[test]
fn theater_is_608_by_420_on_a_32_pt_notch_and_426_on_a_38_pt_notch() {
    assert_eq!(sample(1, 1).theater, size(608.0, 420.0));
    assert_eq!(layout(200.0, 38.0, 1512.0, 1117.0, 1, 1).theater, size(608.0, 426.0));
}

#[test]
fn theater_is_half_the_screen_width_on_a_narrow_screen() {
    assert_eq!(narrow().theater, size(550.0, 387.375));
}

#[test]
fn theater_width_clamps_exactly_at_a_1216_pt_screen() {
    assert_eq!(layout(185.0, 32.0, 1216.0, 982.0, 1, 1).theater, size(608.0, 420.0));
}

#[test]
fn the_stage_does_not_follow_the_screen_width() {
    assert_eq!(narrow().stage, size(640.0, 982.0));
}

#[test]
fn theater_window_is_the_theater_plus_margins() {
    assert_eq!(window_size(&sample(1, 1), WindowMode::Theater), size(640.0, 436.0));
    assert_eq!(window_size(&narrow(), WindowMode::Theater), size(582.0, 403.375));
}

#[test]
fn the_open_window_is_452_wide_on_both_screens() {
    assert_eq!(window_size(&sample(1, 1), WindowMode::Open).width, 452.0);
    assert_eq!(window_size(&narrow(), WindowMode::Open).width, 452.0);
}

#[test]
fn the_open_floor_does_not_apply_to_theater() {
    let mut s = surface(WindowMode::Open, Shape::Open);
    assert_eq!(s.window_size(&tall(500.0)), size(452.0, 516.0));
    s.set(WindowMode::Theater, Shape::Theater);
    assert_eq!(s.window_size(&sample(1, 1)), size(640.0, 436.0));
    // Back to open: the floor starts again.
    s.set(WindowMode::Open, Shape::Open);
    assert_eq!(s.window_size(&sample(1, 1)), size(452.0, 396.0));
}
