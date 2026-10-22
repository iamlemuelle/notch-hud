use super::*;

// Arguments in step order: input, contains, moved, button_down, hit.
fn go(h: &mut Hover, input: Input, contains: bool, moved: bool, button_down: bool, hit: Shape) -> Option<bool> {
    h.step(input, contains, moved, button_down, hit)
}

// A Hover that is in, reached by an Entered with a move.
fn inside(hit: Shape) -> Hover {
    let mut h = Hover::new();
    assert_eq!(go(&mut h, Input::Entered, true, true, false, hit), Some(true));
    h
}

#[test]
fn entered_after_a_move_emits_true() {
    let mut h = Hover::new();
    assert_eq!(go(&mut h, Input::Entered, true, true, false, Shape::Pill), Some(true));
}

#[test]
fn entered_without_a_move_emits_nothing() {
    let mut h = Hover::new();
    assert_eq!(go(&mut h, Input::Entered, true, false, false, Shape::Banner), None);
}

#[test]
fn a_later_moved_with_a_move_emits_true() {
    let mut h = Hover::new();
    assert_eq!(go(&mut h, Input::Entered, true, false, false, Shape::Banner), None);
    assert_eq!(go(&mut h, Input::Moved, true, true, false, Shape::Banner), Some(true));
}

#[test]
fn a_moved_without_a_move_does_not_enter() {
    let mut h = Hover::new();
    assert_eq!(go(&mut h, Input::Moved, true, false, false, Shape::Pill), None);
}

#[test]
fn placed_containing_the_cursor_with_no_prior_hover_stays_out() {
    let mut h = Hover::new();
    assert_eq!(go(&mut h, Input::Placed, true, false, false, Shape::Open), None);
    assert_eq!(go(&mut h, Input::Placed, true, false, false, Shape::Open), None);
}

#[test]
fn placed_not_containing_the_cursor_while_in_emits_false() {
    let mut h = inside(Shape::Open);
    assert_eq!(go(&mut h, Input::Placed, false, false, false, Shape::Pill), Some(false));
}

#[test]
fn placed_containing_the_cursor_while_in_stays_in() {
    let mut h = inside(Shape::Open);
    assert_eq!(go(&mut h, Input::Placed, true, false, false, Shape::Open), None);
}

#[test]
fn exited_while_in_emits_false_whatever_contains_says() {
    let mut h = inside(Shape::Pill);
    assert_eq!(go(&mut h, Input::Exited, true, false, false, Shape::Pill), Some(false));
    let mut h = inside(Shape::Pill);
    assert_eq!(go(&mut h, Input::Exited, false, false, false, Shape::Pill), Some(false));
}

#[test]
fn entered_and_moved_count_as_over_even_when_contains_is_false() {
    let mut h = Hover::new();
    assert_eq!(go(&mut h, Input::Entered, false, true, false, Shape::Pill), Some(true));
    assert_eq!(go(&mut h, Input::Moved, false, true, false, Shape::Pill), None);
    let mut h = Hover::new();
    assert_eq!(go(&mut h, Input::Moved, false, true, false, Shape::Pill), Some(true));
}

#[test]
fn repeated_values_emit_nothing() {
    let mut h = inside(Shape::Open);
    assert_eq!(go(&mut h, Input::Moved, true, true, false, Shape::Open), None);
    assert_eq!(go(&mut h, Input::Moved, true, false, false, Shape::Open), None);
    assert_eq!(go(&mut h, Input::Placed, true, false, false, Shape::Open), None);
    assert_eq!(go(&mut h, Input::Exited, true, false, false, Shape::Open), Some(false));
    assert_eq!(go(&mut h, Input::Exited, true, false, false, Shape::Open), None);
    assert_eq!(go(&mut h, Input::Placed, false, false, false, Shape::Open), None);
}

#[test]
fn re_entering_after_leaving_emits_true_again() {
    let mut h = inside(Shape::Open);
    assert_eq!(go(&mut h, Input::Exited, false, false, false, Shape::Open), Some(false));
    assert_eq!(go(&mut h, Input::Entered, true, true, false, Shape::Open), Some(true));
}

#[test]
fn a_press_while_in_on_open_latches_through_exited_and_up_outside_releases() {
    let mut h = inside(Shape::Open);
    assert_eq!(go(&mut h, Input::Down, true, false, true, Shape::Open), None);
    assert_eq!(go(&mut h, Input::Exited, false, false, true, Shape::Open), None);
    assert_eq!(go(&mut h, Input::Up, false, false, false, Shape::Open), Some(false));
}

#[test]
fn a_press_while_in_on_banner_latches() {
    let mut h = inside(Shape::Banner);
    assert_eq!(go(&mut h, Input::Down, true, false, true, Shape::Banner), None);
    assert_eq!(go(&mut h, Input::Exited, false, false, true, Shape::Banner), None);
    assert_eq!(go(&mut h, Input::Up, false, false, false, Shape::Banner), Some(false));
}

#[test]
fn a_press_while_in_on_peek_latches() {
    let mut h = inside(Shape::Peek);
    assert_eq!(go(&mut h, Input::Down, true, false, true, Shape::Peek), None);
    assert_eq!(go(&mut h, Input::Exited, false, false, true, Shape::Peek), None);
    assert_eq!(go(&mut h, Input::Up, false, false, false, Shape::Peek), Some(false));
}

#[test]
fn an_up_inside_keeps_the_hover_true() {
    let mut h = inside(Shape::Open);
    assert_eq!(go(&mut h, Input::Down, true, false, true, Shape::Open), None);
    assert_eq!(go(&mut h, Input::Up, true, false, false, Shape::Open), None);
    assert_eq!(go(&mut h, Input::Exited, false, false, false, Shape::Open), Some(false));
}

#[test]
fn a_press_while_in_on_pill_does_not_latch() {
    let mut h = inside(Shape::Pill);
    assert_eq!(go(&mut h, Input::Down, true, false, true, Shape::Pill), None);
    assert_eq!(go(&mut h, Input::Exited, false, false, true, Shape::Pill), Some(false));
}

#[test]
fn a_press_while_out_does_not_latch() {
    let mut h = Hover::new();
    assert_eq!(go(&mut h, Input::Down, true, false, true, Shape::Open), None);
    assert_eq!(go(&mut h, Input::Entered, true, true, true, Shape::Open), Some(true));
    assert_eq!(go(&mut h, Input::Exited, false, false, true, Shape::Open), Some(false));
}

#[test]
fn a_press_outside_the_rect_while_in_leaves_without_latching() {
    let mut h = inside(Shape::Open);
    assert_eq!(go(&mut h, Input::Down, false, false, true, Shape::Open), Some(false));
    assert_eq!(go(&mut h, Input::Entered, true, true, true, Shape::Open), Some(true));
    assert_eq!(go(&mut h, Input::Exited, false, false, true, Shape::Open), Some(false));
}

#[test]
fn a_missed_up_is_released_by_the_next_input_with_the_button_up() {
    let mut h = inside(Shape::Open);
    assert_eq!(go(&mut h, Input::Down, true, false, true, Shape::Open), None);
    assert_eq!(go(&mut h, Input::Exited, false, false, true, Shape::Open), None);
    assert_eq!(go(&mut h, Input::Placed, false, false, false, Shape::Open), Some(false));
}

#[test]
fn a_missed_up_is_released_even_when_the_cursor_is_back_over_the_rect() {
    let mut h = inside(Shape::Open);
    assert_eq!(go(&mut h, Input::Down, true, false, true, Shape::Open), None);
    assert_eq!(go(&mut h, Input::Exited, false, false, true, Shape::Open), None);
    assert_eq!(go(&mut h, Input::Placed, true, false, false, Shape::Open), Some(false));
}

#[test]
fn a_moved_with_the_button_down_while_latched_keeps_the_latch() {
    let mut h = inside(Shape::Open);
    assert_eq!(go(&mut h, Input::Down, true, false, true, Shape::Open), None);
    assert_eq!(go(&mut h, Input::Exited, false, false, true, Shape::Open), None);
    assert_eq!(go(&mut h, Input::Moved, false, true, true, Shape::Open), None);
    assert_eq!(go(&mut h, Input::Exited, false, false, true, Shape::Open), None);
    assert_eq!(go(&mut h, Input::Up, false, false, false, Shape::Open), Some(false));
}
