mod layout;
mod media;
mod session;

use std::cell::RefCell;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use objc2::rc::Retained;
use objc2::MainThreadOnly;
use objc2_app_kit::{
    NSBackingStoreType, NSColor, NSEvent, NSPanel, NSScreen, NSWindowCollectionBehavior, NSWindowStyleMask,
};
use objc2_foundation::{MainThreadMarker, NSPoint, NSRect, NSSize};
use tauri::webview::PageLoadEvent;
use tauri::{AppHandle, Emitter, WebviewUrl, WebviewWindow, WebviewWindowBuilder};

use layout::{Layout, Shape, Size, WindowMode};
use media::{Health, Media};
use session::{prune, read_dir, sessions_dir, summarize, Summary};

// kCGMainMenuWindowLevel is 24; +3 matches what notch apps use to clear the menu bar and its status items.
const ABOVE_MENU_BAR: isize = 24 + 3;

/// Notch geometry in AppKit points (bottom-left origin), read afresh on every placement.
struct Notch {
    screen: NSRect,
    x: f64,
    width: f64,
    height: f64,
}

impl Notch {
    fn read(screen: &NSScreen) -> Option<Notch> {
        let height = screen.safeAreaInsets().top;
        if height == 0.0 {
            return None;
        }
        let left = screen.auxiliaryTopLeftArea();
        let right = screen.auxiliaryTopRightArea();
        let x = left.origin.x + left.size.width;
        Some(Notch { screen: screen.frame(), x, width: right.origin.x - x, height })
    }

    fn top(&self) -> f64 {
        self.screen.origin.y + self.screen.size.height
    }

    /// `size` centered on the notch, its top edge at the screen top.
    fn rect(&self, size: Size) -> NSRect {
        let center = self.x + self.width / 2.0;
        NSRect {
            origin: NSPoint { x: center - size.width / 2.0, y: self.top() - size.height },
            size: NSSize { width: size.width, height: size.height },
        }
    }

    fn layout(&self) -> Layout {
        layout::layout(self.width, self.height, self.screen.size.height, latest_row_count(), media_extra_height())
    }
}

/// What the page last asked for with `surface`. `open_height` is the tallest open window since the mode became open.
struct Surface {
    window: WindowMode,
    hit: Shape,
    open_height: f64,
}

/// Whether the cursor was last reported inside the hit rect.
static INSIDE: AtomicBool = AtomicBool::new(false);
/// Left button state at the previous hover poll, and whether a press that started inside a grown shape is held.
static PREV_DOWN: AtomicBool = AtomicBool::new(false);
static PRESS_LATCHED: AtomicBool = AtomicBool::new(false);
static LATEST: Mutex<Option<Summary>> = Mutex::new(None);
static MEDIA: Mutex<Option<Media>> = Mutex::new(None);
static SURFACE: Mutex<Surface> =
    Mutex::new(Surface { window: WindowMode::Pill, hit: Shape::Pill, open_height: 0.0 });
/// The layout the page was last sent, refreshed on every placement.
static LAYOUT: Mutex<Option<Layout>> = Mutex::new(None);

thread_local! {
    /// The visible window: a native non-activating panel that hosts the Tauri window's webview. Created once on
    /// the main thread at startup and kept for the app's lifetime; only the main thread can reach it.
    static PANEL: RefCell<Option<Retained<NSPanel>>> = const { RefCell::new(None) };
}

const PANEL_STYLE: NSWindowStyleMask =
    NSWindowStyleMask::Borderless.union(NSWindowStyleMask::NonactivatingPanel);

/// Main thread only: moves the webview of `window` into a new non-activating panel and shows the panel.
/// A plain window, or one turned into a panel afterwards, activates the app on click; this one does not.
/// The Tauri window stays hidden and must never be shown: it would activate the app on click.
fn adopt_panel(mtm: MainThreadMarker, window: &WebviewWindow) {
    let ptr = window.ns_window().expect("ns_window") as *const objc2_app_kit::NSWindow;
    // SAFETY: Tauri owns this window for the app's lifetime; we only read it here, on the main thread.
    let tao = unsafe { &*ptr };
    let panel = NSPanel::initWithContentRect_styleMask_backing_defer(
        NSPanel::alloc(mtm),
        tao.frame(),
        PANEL_STYLE,
        NSBackingStoreType::Buffered,
        false,
    );
    panel.setLevel(ABOVE_MENU_BAR);
    panel.setCollectionBehavior(
        NSWindowCollectionBehavior::CanJoinAllSpaces
            | NSWindowCollectionBehavior::Stationary
            | NSWindowCollectionBehavior::FullScreenAuxiliary
            | NSWindowCollectionBehavior::IgnoresCycle,
    );
    panel.setHasShadow(false);
    panel.setOpaque(false);
    panel.setBackgroundColor(Some(&NSColor::clearColor()));
    // SAFETY: the panel is held by PANEL, never closed by AppKit on its own.
    unsafe { panel.setReleasedWhenClosed(false) };
    let content = tao.contentView().expect("content view");
    panel.setContentView(Some(&content));
    panel.orderFrontRegardless();
    PANEL.with(|p| *p.borrow_mut() = Some(panel));
}

fn notched_screen(mtm: MainThreadMarker) -> Option<Notch> {
    NSScreen::screens(mtm).iter().find_map(|s| Notch::read(&s))
}

fn latest_row_count() -> usize {
    LATEST.lock().unwrap_or_else(|e| e.into_inner()).as_ref().map_or(0, |s| s.rows.len())
}

/// Extra expanded height for the now playing block: a track, or the "access is off" line.
fn media_extra_height() -> f64 {
    match MEDIA.lock().unwrap_or_else(|e| e.into_inner()).as_ref() {
        Some(Media { track: Some(_), .. }) => 146.0,
        Some(Media { track: None, health: Health::Denied, .. }) => 22.0,
        _ => 0.0,
    }
}

fn mode_name(mode: WindowMode) -> &'static str {
    match mode {
        WindowMode::Pill => "pill",
        WindowMode::Banner => "banner",
        WindowMode::Open => "open",
    }
}

/// Main thread only. Looks the notched screen up again so display changes move the panel, sizes the window for the
/// page's current mode, and emits `layout` when it changed.
fn place(app: &AppHandle) {
    let Some(mtm) = MainThreadMarker::new() else { return };
    let Some(notch) = notched_screen(mtm) else { return };
    let layout = notch.layout();
    let (mode, size) = {
        let mut s = SURFACE.lock().unwrap_or_else(|e| e.into_inner());
        let mut size = layout::window_size(&layout, s.window);
        if s.window == WindowMode::Open {
            s.open_height = s.open_height.max(size.height);
            size.height = s.open_height;
        }
        (s.window, size)
    };
    let target = notch.rect(size);
    PANEL.with(|p| {
        if let Some(panel) = p.borrow().as_ref() {
            let frame = panel.frame();
            if frame != target {
                if frame.size != target.size && trace_on() {
                    eprintln!("window: {} {}x{}", mode_name(mode), size.width, size.height);
                }
                panel.setFrame_display(target, true);
            }
        }
    });
    let changed = {
        let mut last = LAYOUT.lock().unwrap_or_else(|e| e.into_inner());
        let changed = last.as_ref() != Some(&layout);
        *last = Some(layout);
        changed
    };
    if changed {
        let _ = app.emit("layout", layout);
    }
}

fn place_soon(app: &AppHandle) {
    let target = app.clone();
    let _ = app.run_on_main_thread(move || place(&target));
}

// The webview gets no mouse events while this non-key window belongs to a background app,
// so hover is decided natively from the global cursor position, against the shape the page shows.
fn track_hover(app: &AppHandle) {
    let Some(mtm) = MainThreadMarker::new() else { return };
    let Some(notch) = notched_screen(mtm) else { return };
    let Some(layout) = *LAYOUT.lock().unwrap_or_else(|e| e.into_inner()) else { return };
    let hit = SURFACE.lock().unwrap_or_else(|e| e.into_inner()).hit;
    let frame = notch.rect(layout::shape_size(&layout, hit));
    let p = NSEvent::mouseLocation();
    let over = p.x >= frame.origin.x
        && p.x <= frame.origin.x + frame.size.width
        && p.y >= frame.origin.y
        && p.y <= frame.origin.y + frame.size.height;
    // A press that starts inside a grown shape keeps hover on until release, so a seek drag that overshoots the
    // bar does not collapse the panel. A press that starts elsewhere is never latched.
    let down = NSEvent::pressedMouseButtons() & 1 != 0;
    let pressed = down && !PREV_DOWN.swap(down, Ordering::Relaxed);
    if !down {
        PREV_DOWN.store(false, Ordering::Relaxed);
        PRESS_LATCHED.store(false, Ordering::Relaxed);
    } else if pressed && over && hit != Shape::Pill {
        PRESS_LATCHED.store(true, Ordering::Relaxed);
    }
    let inside = over || PRESS_LATCHED.load(Ordering::Relaxed);
    if inside != INSIDE.swap(inside, Ordering::Relaxed) {
        if trace_on() {
            eprintln!("hover: {}", if inside { "in" } else { "out" });
        }
        let _ = app.emit("hover", inside);
    }
}

fn poll_sessions(app: &AppHandle) {
    let now = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs());
    let dir = sessions_dir();
    prune(&dir, now);
    let summary = summarize(&read_dir(&dir), now);
    {
        let mut latest = LATEST.lock().unwrap_or_else(|e| e.into_inner());
        if latest.as_ref() != Some(&summary) {
            *latest = Some(summary.clone());
            let _ = app.emit("sessions", &summary);
        }
    }
    place_soon(app);
}

#[tauri::command]
fn sessions() -> Option<Summary> {
    LATEST.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

fn trace_on() -> bool {
    std::env::var_os("NOTCH_TRACE").is_some_and(|v| !v.is_empty())
}

fn trace_line(m: &Media) -> String {
    let via = serde_json::to_value(m.via).unwrap_or_default();
    let health = serde_json::to_value(m.health).unwrap_or_default();
    let (via, health) = (via.as_str().unwrap_or("?"), health.as_str().unwrap_or("?"));
    match &m.track {
        Some(t) => {
            let state = if t.playing { "playing" } else { "paused" };
            format!("media: {via} {health} {state} \"{}\" ({})", t.title, t.app)
        }
        None => format!("media: {via} {health} nothing"),
    }
}

fn on_media(app: &AppHandle, media: Media) {
    {
        let mut latest = MEDIA.lock().unwrap_or_else(|e| e.into_inner());
        if latest.as_ref() == Some(&media) {
            return;
        }
        *latest = Some(media.clone());
    }
    if trace_on() {
        eprintln!("{}", trace_line(&media));
    }
    let _ = app.emit("media", &media);
    place_soon(app);
}

/// Parses, routes against the latest media and applies off the main thread.
#[tauri::command]
async fn act(action: serde_json::Value) -> Result<(), String> {
    let trace = trace_on();
    let action = match media::parse_action(&action) {
        Ok(action) => action,
        Err(reason) => {
            if trace {
                eprintln!("media: act rejected: {reason}");
            }
            return Err(reason);
        }
    };
    let latest = MEDIA.lock().unwrap_or_else(|e| e.into_inner()).clone();
    let label = media::clamp(latest.as_ref(), action).label();
    let result = match media::route(latest.as_ref(), action) {
        Ok(route) => tauri::async_runtime::spawn_blocking(move || media::apply(&route))
            .await
            .unwrap_or_else(|e| Err(format!("act task: {e}"))),
        Err(reason) => Err(reason),
    };
    if trace {
        match &result {
            Ok(()) => eprintln!("media: act {label} ok"),
            Err(reason) => eprintln!("media: act {label} failed: {reason}"),
        }
    }
    result
}

#[tauri::command]
fn media() -> Option<Media> {
    MEDIA.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

#[tauri::command]
fn layout() -> Option<Layout> {
    *LAYOUT.lock().unwrap_or_else(|e| e.into_inner())
}

/// The page's window mode and hit shape (CONTRACT.md section 8). Serde rejects unknown values before this runs.
#[tauri::command]
fn surface(app: AppHandle, window: WindowMode, hit: Shape) {
    {
        let mut s = SURFACE.lock().unwrap_or_else(|e| e.into_inner());
        if s.window != window {
            s.open_height = 0.0;
        }
        s.window = window;
        s.hit = hit;
    }
    place_soon(&app);
}

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![sessions, media, act, layout, surface])
        .setup(|app| {
            let mtm = MainThreadMarker::new().expect("main thread");
            if notched_screen(mtm).is_none() {
                eprintln!("notch-hud: no notched display");
                std::process::exit(0);
            }
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);
            let window = WebviewWindowBuilder::new(app, "notch", WebviewUrl::App("index.html".into()))
                .decorations(false)
                .transparent(true)
                .resizable(false)
                .skip_taskbar(true)
                .visible(false)
                .accept_first_mouse(true)
                .on_page_load(|_, payload| {
                    if payload.event() == PageLoadEvent::Finished && trace_on() {
                        eprintln!("page: loaded");
                    }
                })
                .build()?;
            adopt_panel(mtm, &window);
            place(app.handle());

            drop(window);

            let hover = app.handle().clone();
            std::thread::spawn(move || loop {
                std::thread::sleep(Duration::from_millis(50));
                let target = hover.clone();
                let _ = hover.run_on_main_thread(move || track_hover(&target));
            });
            let poll = app.handle().clone();
            std::thread::spawn(move || loop {
                poll_sessions(&poll);
                std::thread::sleep(Duration::from_secs(1));
            });
            let media_app = app.handle().clone();
            std::thread::spawn(move || media::run(move |m| on_media(&media_app, m)));
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("tauri run");
}
