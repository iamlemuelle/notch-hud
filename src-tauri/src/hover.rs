//! Hover state from the tracking area's and the mouse monitor's events (CONTRACT.md sections 3 and 8). Pure.

use crate::layout::Shape;

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

    pub fn step(&mut self, input: Input, contains: bool, moved: bool, button_down: bool, hit: Shape) -> Option<bool> {
        let _ = (input, contains, moved, button_down, hit);
        todo!("hover step")
    }
}

#[cfg(test)]
#[path = "hover_tests.rs"]
mod tests;
