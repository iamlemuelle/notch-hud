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

    /// `contains`: the cursor is in the hit rect; `moved`: it differs from where it was at the previous pointer input.
    /// Returns the new `hover` value when it changed. Only `Up` unlatches: the live button state is up before AppKit
    /// dispatches the release, so unlatching on it could send hover out ahead of the page's `pointerup`.
    pub fn step(&mut self, input: Input, contains: bool, moved: bool, hit: Shape) -> Option<bool> {
        let motion = matches!(input, Input::Entered | Input::Moved);
        let over = match input {
            Input::Entered => true,
            Input::Exited => false,
            Input::Moved | Input::Down | Input::Up | Input::Placed => contains,
        };
        self.hover_in = next_hover(self.hover_in, over, motion && moved);
        match input {
            Input::Down if self.hover_in && hit != Shape::Pill => self.latched = true,
            Input::Up => self.latched = false,
            _ => {}
        }
        let value = self.hover_in || self.latched;
        (value != std::mem::replace(&mut self.last, value)).then_some(value)
    }

    pub fn latched(&self) -> bool {
        self.latched
    }
}

#[cfg(test)]
#[path = "hover_tests.rs"]
mod tests;
