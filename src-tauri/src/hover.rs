//! Hover state from the tracking area's and the mouse monitor's events (CONTRACT.md sections 3 and 8). Pure.

use crate::layout::{next_hover, Shape};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Input {
    Entered,
    Moved,
    Exited,
    Down,
    Up,
    Placed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Hover {
    hover_in: bool,
    latched: bool,
    last: bool,
}

impl Hover {
    pub const fn new() -> Hover {
        Hover { hover_in: false, latched: false, last: false }
    }

    /// `contains`: the cursor is in the hit rect; `moved`: it differs from the previous input's; `button_down`: the
    /// left button is down now. Returns the new `hover` value when it changed.
    pub fn step(&mut self, input: Input, contains: bool, moved: bool, button_down: bool, hit: Shape) -> Option<bool> {
        let pointer = matches!(input, Input::Entered | Input::Moved);
        let over = match input {
            Input::Entered | Input::Moved => true,
            Input::Exited => false,
            Input::Down | Input::Up | Input::Placed => contains,
        };
        self.hover_in = next_hover(self.hover_in, over, pointer && moved);
        match input {
            Input::Down if self.hover_in && hit != Shape::Pill => self.latched = true,
            Input::Down => {}
            Input::Up => self.latched = false,
            _ if !button_down => self.latched = false,
            _ => {}
        }
        let value = self.hover_in || self.latched;
        (value != std::mem::replace(&mut self.last, value)).then_some(value)
    }
}

#[cfg(test)]
#[path = "hover_tests.rs"]
mod tests;
