use super::*;

// A Hover that is in, reached by an Entered with a move.
fn inside(hit: Shape) -> Hover {
    let mut h = Hover::new();
    assert_eq!(h.step(Input::Entered, true, true, hit), Some(true));
    h
}

#[test]
fn entered_after_a_move_emits_true() {
    let mut h = Hover::new();
    assert_eq!(h.step(Input::Entered, true, true, Shape::Pill), Some(true));
}

#[test]
fn entered_without_a_move_emits_nothing() {
    let mut h = Hover::new();
    assert_eq!(h.step(Input::Entered, true, false, Shape::Banner), None);
}

#[test]
fn a_later_moved_with_a_move_emits_true() {
    let mut h = Hover::new();
    assert_eq!(h.step(Input::Entered, true, false, Shape::Banner), None);
    assert_eq!(h.step(Input::Moved, true, true, Shape::Banner), Some(true));
}

#[test]
fn a_moved_without_a_move_does_not_enter() {
    let mut h = Hover::new();
    assert_eq!(h.step(Input::Moved, true, false, Shape::Pill), None);
}

#[test]
fn placed_containing_the_cursor_with_no_prior_hover_stays_out() {
    let mut h = Hover::new();
    assert_eq!(h.step(Input::Placed, true, false, Shape::Open), None);
    assert_eq!(h.step(Input::Placed, true, false, Shape::Open), None);
}

#[test]
fn placed_not_containing_the_cursor_while_in_emits_false() {
    let mut h = inside(Shape::Open);
    assert_eq!(h.step(Input::Placed, false, false, Shape::Pill), Some(false));
}

#[test]
fn placed_containing_the_cursor_while_in_stays_in() {
    let mut h = inside(Shape::Open);
    assert_eq!(h.step(Input::Placed, true, false, Shape::Open), None);
}

#[test]
fn exited_while_in_emits_false_whatever_contains_says() {
    let mut h = inside(Shape::Pill);
    assert_eq!(h.step(Input::Exited, true, false, Shape::Pill), Some(false));
    let mut h = inside(Shape::Pill);
    assert_eq!(h.step(Input::Exited, false, false, Shape::Pill), Some(false));
}

#[test]
fn entered_and_moved_count_as_over_even_when_contains_is_false() {
    let mut h = Hover::new();
    assert_eq!(h.step(Input::Entered, false, true, Shape::Pill), Some(true));
    assert_eq!(h.step(Input::Moved, false, true, Shape::Pill), None);
    let mut h = Hover::new();
    assert_eq!(h.step(Input::Moved, false, true, Shape::Pill), Some(true));
}

#[test]
fn repeated_values_emit_nothing() {
    let mut h = inside(Shape::Open);
    assert_eq!(h.step(Input::Moved, true, true, Shape::Open), None);
    assert_eq!(h.step(Input::Moved, true, false, Shape::Open), None);
    assert_eq!(h.step(Input::Placed, true, false, Shape::Open), None);
    assert_eq!(h.step(Input::Exited, true, false, Shape::Open), Some(false));
    assert_eq!(h.step(Input::Exited, true, false, Shape::Open), None);
    assert_eq!(h.step(Input::Placed, false, false, Shape::Open), None);
}

#[test]
fn re_entering_after_leaving_emits_true_again() {
    let mut h = inside(Shape::Open);
    assert_eq!(h.step(Input::Exited, false, false, Shape::Open), Some(false));
    assert_eq!(h.step(Input::Entered, true, true, Shape::Open), Some(true));
}

#[test]
fn a_press_while_in_on_open_latches_through_exited_and_up_outside_releases() {
    let mut h = inside(Shape::Open);
    assert_eq!(h.step(Input::Down, true, false, Shape::Open), None);
    assert_eq!(h.step(Input::Exited, false, false, Shape::Open), None);
    assert_eq!(h.step(Input::Up, false, false, Shape::Open), Some(false));
}

#[test]
fn a_press_while_in_on_banner_latches() {
    let mut h = inside(Shape::Banner);
    assert_eq!(h.step(Input::Down, true, false, Shape::Banner), None);
    assert_eq!(h.step(Input::Exited, false, false, Shape::Banner), None);
    assert_eq!(h.step(Input::Up, false, false, Shape::Banner), Some(false));
}

#[test]
fn a_press_while_in_on_peek_latches() {
    let mut h = inside(Shape::Peek);
    assert_eq!(h.step(Input::Down, true, false, Shape::Peek), None);
    assert_eq!(h.step(Input::Exited, false, false, Shape::Peek), None);
    assert_eq!(h.step(Input::Up, false, false, Shape::Peek), Some(false));
}

#[test]
fn an_up_inside_keeps_the_hover_true() {
    let mut h = inside(Shape::Open);
    assert_eq!(h.step(Input::Down, true, false, Shape::Open), None);
    assert_eq!(h.step(Input::Up, true, false, Shape::Open), None);
    assert_eq!(h.step(Input::Exited, false, false, Shape::Open), Some(false));
}

#[test]
fn a_press_while_in_on_pill_does_not_latch() {
    let mut h = inside(Shape::Pill);
    assert_eq!(h.step(Input::Down, true, false, Shape::Pill), None);
    assert_eq!(h.step(Input::Exited, false, false, Shape::Pill), Some(false));
}

#[test]
fn a_press_while_out_does_not_latch() {
    let mut h = Hover::new();
    assert_eq!(h.step(Input::Down, true, false, Shape::Open), None);
    assert_eq!(h.step(Input::Entered, true, true, Shape::Open), Some(true));
    assert_eq!(h.step(Input::Exited, false, false, Shape::Open), Some(false));
}

#[test]
fn a_press_outside_the_rect_while_in_leaves_without_latching() {
    let mut h = inside(Shape::Open);
    assert_eq!(h.step(Input::Down, false, false, Shape::Open), Some(false));
    assert_eq!(h.step(Input::Entered, true, true, Shape::Open), Some(true));
    assert_eq!(h.step(Input::Exited, false, false, Shape::Open), Some(false));
}

#[test]
fn a_moved_with_the_button_down_while_latched_keeps_the_latch() {
    let mut h = inside(Shape::Open);
    assert_eq!(h.step(Input::Down, true, false, Shape::Open), None);
    assert_eq!(h.step(Input::Exited, false, false, Shape::Open), None);
    assert_eq!(h.step(Input::Moved, false, true, Shape::Open), None);
    assert_eq!(h.step(Input::Exited, false, false, Shape::Open), None);
    assert_eq!(h.step(Input::Up, false, false, Shape::Open), Some(false));
}

#[test]
fn while_latched_nothing_but_up_unlatches() {
    let mut h = inside(Shape::Open);
    assert_eq!(h.step(Input::Down, true, false, Shape::Open), None);
    assert_eq!(h.step(Input::Exited, false, false, Shape::Open), None);
    assert_eq!(h.step(Input::Placed, false, false, Shape::Open), None);
    assert_eq!(h.step(Input::Moved, false, false, Shape::Open), None);
    assert_eq!(h.step(Input::Entered, false, false, Shape::Open), None);
    assert_eq!(h.step(Input::Exited, true, false, Shape::Open), None);
    assert_eq!(h.step(Input::Placed, true, false, Shape::Open), None);
    assert_eq!(h.step(Input::Up, false, false, Shape::Open), Some(false));
}

#[test]
fn an_up_outside_the_rect_without_an_exited_releases() {
    let mut h = inside(Shape::Open);
    assert_eq!(h.step(Input::Down, true, false, Shape::Open), None);
    assert_eq!(h.step(Input::Up, false, false, Shape::Open), Some(false));
}

#[test]
fn latched_is_false_until_a_latching_down_and_false_again_after_up() {
    assert!(!Hover::new().latched());
    let mut h = inside(Shape::Open);
    assert!(!h.latched());
    assert_eq!(h.step(Input::Down, true, false, Shape::Open), None);
    assert!(h.latched());
    assert_eq!(h.step(Input::Up, false, false, Shape::Open), Some(false));
    assert!(!h.latched());
}

#[test]
fn latched_stays_false_for_a_down_on_pill_or_while_out() {
    let mut h = inside(Shape::Pill);
    assert_eq!(h.step(Input::Down, true, false, Shape::Pill), None);
    assert!(!h.latched());
    let mut h = Hover::new();
    assert_eq!(h.step(Input::Down, true, false, Shape::Open), None);
    assert!(!h.latched());
}
