//! Panel geometry in points. Every rect is centered on the notch with its top edge at the screen top, so a size is
//! all a rect needs. The page draws the shapes; Rust sizes the window and hit-tests against them.

#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize)]
pub struct Size {
    pub width: f64,
    pub height: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize)]
pub struct Layout {
    pub stage: Size,
    pub pill: Size,
    pub peek: Size,
    pub banner: Size,
    pub open: Size,
    pub ear: f64,
}

/// A shape the page can show, used as the hit rect.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Shape {
    Pill,
    Peek,
    Banner,
    Open,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WindowMode {
    Pill,
    Banner,
    Open,
}

pub const EAR: f64 = 60.0;
pub const EXPANDED_WIDTH: f64 = 420.0;
pub const BANNER_WIDTH: f64 = 380.0;
/// Room around the banner and open shapes for the spring's overshoot.
pub const MARGIN: f64 = 16.0;

/// The music block (196), the session header (18), a 4 pt gap, one 22 pt row per shown session row and 14 below.
/// Shown rows: the summary's session rows, plus a "N more" row when the total exceeds them; one "No Claude Code
/// sessions" row at zero.
pub fn expanded_height(rows: usize, total: usize) -> f64 {
    let shown_rows = (rows + usize::from(total > rows)).max(1);
    (196 + 18 + 4 + 22 * shown_rows + 14) as f64
}

pub fn layout(notch_width: f64, notch_height: f64, screen_height: f64, rows: usize, total: usize) -> Layout {
    let pill = Size { width: notch_width + 2.0 * EAR, height: notch_height };
    Layout {
        stage: Size { width: EXPANDED_WIDTH + 2.0 * MARGIN, height: screen_height },
        pill,
        peek: Size { width: pill.width + 4.0, height: pill.height + 4.0 },
        banner: Size { width: BANNER_WIDTH, height: notch_height + 46.0 },
        open: Size { width: EXPANDED_WIDTH, height: expanded_height(rows, total) },
        ear: EAR,
    }
}

pub fn shape_size(layout: &Layout, shape: Shape) -> Size {
    match shape {
        Shape::Pill => layout.pill,
        Shape::Peek => layout.peek,
        Shape::Banner => layout.banner,
        Shape::Open => layout.open,
    }
}

pub fn window_size(layout: &Layout, mode: WindowMode) -> Size {
    match mode {
        WindowMode::Pill => layout.pill,
        WindowMode::Banner => Size {
            width: layout.banner.width + 2.0 * MARGIN,
            height: layout.banner.height + MARGIN,
        },
        WindowMode::Open => Size { width: layout.stage.width, height: layout.open.height + MARGIN },
    }
}

/// What the page last asked for with `surface`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Surface {
    pub window: WindowMode,
    pub hit: Shape,
    /// The tallest open window returned since the mode became open.
    open_floor: f64,
}

impl Surface {
    pub const fn new() -> Surface {
        Surface { window: WindowMode::Pill, hit: Shape::Pill, open_floor: 0.0 }
    }

    pub fn set(&mut self, window: WindowMode, hit: Shape) {
        if window != self.window {
            self.open_floor = 0.0;
        }
        self.window = window;
        self.hit = hit;
    }

    /// While open, never shorter than before, so a row disappearing mid-spring never cuts the shape.
    pub fn window_size(&mut self, layout: &Layout) -> Size {
        let mut size = window_size(layout, self.window);
        if self.window == WindowMode::Open {
            self.open_floor = self.open_floor.max(size.height);
            size.height = self.open_floor;
        }
        size
    }
}

/// AppKit coordinates (y up): inside the rect of `size` centered on `center_x` with its top edge at `top`.
pub fn hit_contains(size: Size, center_x: f64, top: f64, x: f64, y: f64) -> bool {
    (x - center_x).abs() <= size.width / 2.0 && y <= top && y >= top - size.height
}

/// Entering needs the cursor to move, so a shape growing under a still cursor is not a hover; leaving does not.
pub fn next_hover(was_in: bool, over: bool, moved: bool) -> bool {
    over && (was_in || moved)
}

#[cfg(test)]
#[path = "layout_tests.rs"]
mod tests;
