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

pub fn layout(notch_width: f64, notch_height: f64, screen_height: f64, rows: usize, media_extra: f64) -> Layout {
    let pill = Size { width: notch_width + 2.0 * EAR, height: notch_height };
    Layout {
        stage: Size { width: EXPANDED_WIDTH + 2.0 * MARGIN, height: screen_height },
        pill,
        peek: Size { width: pill.width + 4.0, height: pill.height + 4.0 },
        banner: Size { width: BANNER_WIDTH, height: notch_height + 46.0 },
        open: Size { width: EXPANDED_WIDTH, height: 44.0 + media_extra + 22.0 * rows.max(1) as f64 + 14.0 },
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

#[cfg(test)]
#[path = "layout_tests.rs"]
mod tests;
