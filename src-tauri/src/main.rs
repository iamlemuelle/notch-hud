mod hover;
mod layout;
mod media;
mod session;

use std::cell::{Cell, OnceCell, RefCell};
use std::path::Path;
use std::ptr::NonNull;
use std::sync::{mpsc, Mutex, OnceLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{define_class, msg_send, sel, AnyThread, MainThreadOnly};
use objc2_app_kit::{
    NSBackingStoreType, NSColor, NSEvent, NSEventMask, NSEventType, NSPanel, NSScreen, NSTrackingArea,
    NSTrackingAreaOptions, NSWindowCollectionBehavior, NSWindowStyleMask,
};
use objc2_foundation::{MainThreadMarker, NSObject, NSPoint, NSRect, NSSize};
use tauri::webview::PageLoadEvent;
use tauri::{AppHandle, Emitter, WebviewUrl, WebviewWindow, WebviewWindowBuilder};

use hover::{Hover, Input};
use layout::{Layout, Shape, Size, Surface, WindowMode};
use media::Media;
use notify::{RecursiveMode, Watcher};
use session::{next_wake, prune, read_dir, sessions_dir, summarize, Summary};

/// The sessions safety net: a missed file event, a recreated directory, a display change (CONTRACT.md section 3).
const RESCAN: Duration = Duration::from_secs(5);

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
        {
        let (rows, total) = latest_counts();
        layout::layout(self.width, self.height, self.screen.size.height, rows, total)
    }
    }
}

/// For `emit` from AppKit callbacks, which carry no handle. Set once at startup.
static APP: OnceLock<AppHandle> = OnceLock::new();
static LATEST: Mutex<Option<Summary>> = Mutex::new(None);
static MEDIA: Mutex<Option<Media>> = Mutex::new(None);
static SURFACE: Mutex<Surface> = Mutex::new(Surface::new());
/// The layout the page was last sent, refreshed on every placement.
static LAYOUT: Mutex<Option<Layout>> = Mutex::new(None);

thread_local! {
    /// The visible window: a native non-activating panel that hosts the Tauri window's webview. Created once on
    /// the main thread at startup and kept for the app's lifetime; only the main thread can reach it.
    static PANEL: RefCell<Option<Retained<NSPanel>>> = const { RefCell::new(None) };
    /// The cursor at the previous hover input.
    static LAST_CURSOR: Cell<Option<(f64, f64)>> = const { Cell::new(None) };
    static HOVER: Cell<Hover> = const { Cell::new(Hover::new()) };
    /// The tracking area on the panel's content view, replaced on every placement and every release.
    static AREA: RefCell<Option<Retained<NSTrackingArea>>> = const { RefCell::new(None) };
    /// The area's owner. AppKit does not retain it, so it lives here for the app's lifetime.
    static OWNER: OnceCell<Retained<HoverOwner>> = const { OnceCell::new() };
    /// The left-button monitor, kept for the app's lifetime.
    static MONITOR: RefCell<Option<Retained<AnyObject>>> = const { RefCell::new(None) };
}

const TRACKING: NSTrackingAreaOptions = NSTrackingAreaOptions::MouseEnteredAndExited
    .union(NSTrackingAreaOptions::MouseMoved)
    .union(NSTrackingAreaOptions::ActiveAlways)
    .union(NSTrackingAreaOptions::EnabledDuringMouseDrag);

define_class!(
    // SAFETY: NSObject has no subclassing requirements, and HoverOwner has no Drop.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "NotchHudHoverOwner"]
    struct HoverOwner;

    impl HoverOwner {
        #[unsafe(method(mouseEntered:))]
        fn mouse_entered(&self, _event: &NSEvent) {
            track_hover(Input::Entered);
        }

        #[unsafe(method(mouseMoved:))]
        fn mouse_moved(&self, _event: &NSEvent) {
            track_hover(Input::Moved);
        }

        #[unsafe(method(mouseExited:))]
        fn mouse_exited(&self, _event: &NSEvent) {
            track_hover(Input::Exited);
        }

        #[unsafe(method(released))]
        fn released(&self) {
            track_hover(Input::Up);
        }
    }
);

impl HoverOwner {
    /// Main thread only: the one owner, created on first use.
    fn shared(mtm: MainThreadMarker) -> Retained<HoverOwner> {
        OWNER.with(|o| {
            o.get_or_init(|| {
                let this = HoverOwner::alloc(mtm).set_ivars(());
                // SAFETY: NSObject's designated initializer.
                unsafe { msg_send![super(this), init] }
            })
            .clone()
        })
    }
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

/// Main thread only. The panel's frame, or `None` before it exists.
fn panel_frame() -> Option<NSRect> {
    PANEL.with(|p| p.borrow().as_ref().map(|panel| panel.frame()))
}

fn notched_screen(mtm: MainThreadMarker) -> Option<Notch> {
    NSScreen::screens(mtm).iter().find_map(|s| Notch::read(&s))
}

fn latest_counts() -> (usize, usize) {
    LATEST.lock().unwrap_or_else(|e| e.into_inner()).as_ref().map_or((0, 0), |s| (s.rows.len(), s.total))
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
        let size = s.window_size(&layout);
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
    track_hover(Input::Placed);
}

fn place_soon(app: &AppHandle) {
    let target = app.clone();
    let _ = app.run_on_main_thread(move || place(&target));
}

// The webview gets no mouse events while this non-key window belongs to a background app, so hover is decided
// natively, against the shape the page shows: a tracking area and a left-button monitor feed `Hover`.
/// Main thread only. Applies one hover input and emits `hover` when it changes. After a placement or a release it
/// replaces the tracking area, so AppKit's idea of inside matches the state (it can drift during a drag).
fn track_hover(input: Input) {
    let Some(app) = APP.get() else { return };
    let Some(frame) = panel_frame() else { return };
    let Some(layout) = *LAYOUT.lock().unwrap_or_else(|e| e.into_inner()) else { return };
    let hit = SURFACE.lock().unwrap_or_else(|e| e.into_inner()).hit;
    let size = layout::shape_size(&layout, hit);
    // Every rect shares the window's horizontal center and top edge, so the hit rect follows from the frame.
    let center = frame.origin.x + frame.size.width / 2.0;
    let top = frame.origin.y + frame.size.height;
    let p = NSEvent::mouseLocation();
    let contains = layout::hit_contains(size, center, top, p.x, p.y);
    let moved = LAST_CURSOR.with(|c| c.replace(Some((p.x, p.y)))) != Some((p.x, p.y));
    let down = NSEvent::pressedMouseButtons() & 1 != 0;
    let mut hover = HOVER.get();
    let changed = hover.step(input, contains, moved, down, hit);
    HOVER.set(hover);
    if let Some(inside) = changed {
        if trace_on() {
            eprintln!("hover: {}", if inside { "in" } else { "out" });
        }
        let _ = app.emit("hover", inside);
    }
    if matches!(input, Input::Placed | Input::Up) {
        track_area(frame, size, contains);
    }
}

/// Main thread only. Replaces the tracking area with one over the hit rect of `size` in a panel of `frame`, assumed
/// inside when `inside`, so an area appearing under a still cursor sends no `mouseEntered:`.
fn track_area(frame: NSRect, size: Size, inside: bool) {
    let Some(mtm) = MainThreadMarker::new() else { return };
    let Some(view) = PANEL.with(|p| p.borrow().as_ref().and_then(|panel| panel.contentView())) else { return };
    let in_window = NSRect {
        origin: NSPoint { x: (frame.size.width - size.width) / 2.0, y: frame.size.height - size.height },
        size: NSSize { width: size.width, height: size.height },
    };
    let rect = view.convertRect_fromView(in_window, None);
    let options = if inside { TRACKING.union(NSTrackingAreaOptions::AssumeInside) } else { TRACKING };
    let owner = HoverOwner::shared(mtm);
    let owner: &AnyObject = &owner;
    // SAFETY: the owner implements the three mouse methods the options ask for and outlives the area (OWNER).
    let area = unsafe {
        NSTrackingArea::initWithRect_options_owner_userInfo(NSTrackingArea::alloc(), rect, options, Some(owner), None)
    };
    if let Some(old) = AREA.with(|a| a.replace(Some(area.clone()))) {
        view.removeTrackingArea(&old);
    }
    view.addTrackingArea(&area);
}

/// Main thread only. Feeds left presses and releases on the panel to hover; a press in another app never gets here.
/// The monitor runs before AppKit dispatches the event. A release is applied on the next run-loop pass instead, after
/// the webview has passed the mouse-up on, so the page sees its `pointerup` before the `hover` out a release can emit
/// (it ends a seek drag that sees hover out first without seeking).
fn monitor_presses(mtm: MainThreadMarker) {
    let owner = HoverOwner::shared(mtm);
    let handler = block2::RcBlock::new(move |event: NonNull<NSEvent>| -> *mut NSEvent {
        // SAFETY: AppKit passes a valid event for the duration of the call.
        if unsafe { event.as_ref() }.r#type() == NSEventType::LeftMouseDown {
            track_hover(Input::Down);
        } else {
            let nothing: Option<&AnyObject> = None;
            // SAFETY: `released` takes no argument; OWNER keeps the owner alive.
            let _: () = unsafe {
                msg_send![&*owner, performSelectorOnMainThread: sel!(released), withObject: nothing, waitUntilDone: false]
            };
        }
        event.as_ptr()
    });
    let mask = NSEventMask::LeftMouseDown | NSEventMask::LeftMouseUp;
    // SAFETY: the handler returns the event it was given.
    let monitor = unsafe { NSEvent::addLocalMonitorForEventsMatchingMask_handler(mask, &handler) };
    MONITOR.with(|m| *m.borrow_mut() = monitor);
}

/// Refreshes the summary and returns the deadline `next_wake` gives for these records.
fn refresh_sessions(app: &AppHandle, dir: &Path) -> Option<u64> {
    let now = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs());
    prune(dir, now);
    let records = read_dir(dir);
    let summary = summarize(&records, now);
    {
        let mut latest = LATEST.lock().unwrap_or_else(|e| e.into_inner());
        if latest.as_ref() != Some(&summary) {
            if trace_on() {
                eprintln!("sessions: {}", serde_json::to_string(&summary).unwrap_or_default());
            }
            *latest = Some(summary.clone());
            let _ = app.emit("sessions", &summary);
        }
    }
    place_soon(app);
    next_wake(&records, now)
}

/// How long to sleep before the next refresh, and the trace cause if no file event comes first: `deadline` (unix
/// seconds) when it is within `RESCAN`, else the rescan.
fn wait_for(deadline: Option<u64>) -> (Duration, &'static str) {
    let until = deadline.and_then(|secs| {
        let at = UNIX_EPOCH.checked_add(Duration::from_secs(secs))?;
        Some(at.duration_since(SystemTime::now()).unwrap_or(Duration::ZERO))
    });
    match until {
        Some(wait) if wait < RESCAN => (wait, "deadline"),
        _ => (RESCAN, "rescan"),
    }
}

/// Refreshes on every file event in the sessions directory, at the next deadline, and at least every `RESCAN`.
fn watch_sessions(app: &AppHandle) {
    let dir = sessions_dir();
    let _ = std::fs::create_dir_all(&dir);
    let (tx, rx) = mpsc::channel();
    // Held here so `rx` never disconnects when the watcher could not be created, which would turn `recv_timeout`
    // into a busy loop.
    let _keep = tx.clone();
    let mut watcher = notify::recommended_watcher(move |_| {
        let _ = tx.send(());
    });
    let mut watching = false;
    let mut wake = "start";
    loop {
        if !watching {
            let result = match watcher.as_mut() {
                Ok(w) => w.watch(&dir, RecursiveMode::NonRecursive),
                Err(e) => Err(notify::Error::generic(&e.to_string())),
            };
            if let Err(e) = &result {
                if trace_on() {
                    eprintln!("watch: failed: {e}");
                }
            }
            watching = result.is_ok();
        }
        if trace_on() {
            eprintln!("refresh: {wake}");
        }
        let deadline = refresh_sessions(app, &dir);
        let (wait, timeout) = wait_for(deadline);
        wake = match rx.recv_timeout(wait) {
            Ok(()) => {
                while rx.try_recv().is_ok() {}
                "event"
            }
            Err(_) => timeout,
        };
    }
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
/// Synchronous, so Tauri runs it on the main thread in arrival order, and it returns once the panel is placed: the
/// page waits for that before animating into a bigger window.
#[tauri::command]
fn surface(app: AppHandle, window: WindowMode, hit: Shape) {
    SURFACE.lock().unwrap_or_else(|e| e.into_inner()).set(window, hit);
    place(&app);
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
            let _ = APP.set(app.handle().clone());
            adopt_panel(mtm, &window);
            monitor_presses(mtm);
            place(app.handle());

            drop(window);

            let sessions_app = app.handle().clone();
            std::thread::spawn(move || watch_sessions(&sessions_app));
            let media_app = app.handle().clone();
            std::thread::spawn(move || media::run(move |m| on_media(&media_app, m)));
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("tauri run");
}
