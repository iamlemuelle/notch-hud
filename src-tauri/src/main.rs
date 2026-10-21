mod session;

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use objc2_app_kit::{NSEvent, NSScreen, NSWindow, NSWindowCollectionBehavior};
use objc2_foundation::{MainThreadMarker, NSPoint, NSRect, NSSize};
use tauri::{Emitter, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder};

use session::{read_dir, sessions_dir, summarize, Summary};

// kCGMainMenuWindowLevel is 24; +3 matches what notch apps use to clear the menu bar and its status items.
const ABOVE_MENU_BAR: isize = 24 + 3;
const EAR: f64 = 60.0;
const EXPANDED_WIDTH: f64 = 420.0;

/// Notch geometry in AppKit points (bottom-left origin), read once at startup.
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

    fn panel(&self, expanded_rows: Option<usize>) -> NSRect {
        let size = match expanded_rows {
            Some(rows) => NSSize {
                width: EXPANDED_WIDTH,
                height: (44 + 22 * rows.max(1) + 14) as f64,
            },
            None => NSSize { width: self.width + 2.0 * EAR, height: self.height },
        };
        let center = self.x + self.width / 2.0;
        NSRect {
            origin: NSPoint { x: center - size.width / 2.0, y: self.top() - size.height },
            size,
        }
    }
}

static NOTCH: OnceLock<Notch> = OnceLock::new();
static EXPANDED_NOW: AtomicBool = AtomicBool::new(false);
static ROW_COUNT: AtomicUsize = AtomicUsize::new(0);
static LATEST: Mutex<Option<Summary>> = Mutex::new(None);

fn ns_window(window: &WebviewWindow) -> &NSWindow {
    let ptr = window.ns_window().expect("ns_window") as *const NSWindow;
    unsafe { &*ptr }
}

fn notched_screen(mtm: MainThreadMarker) -> Option<Notch> {
    NSScreen::screens(mtm).iter().find_map(|s| Notch::read(&s))
}

fn place(window: &WebviewWindow, notch: &Notch, expanded: bool) {
    let rows = expanded.then(|| ROW_COUNT.load(Ordering::Relaxed));
    ns_window(window).setFrame_display(notch.panel(rows), true);
}

// The webview gets no mouse events while this non-key window belongs to a background app,
// so hover is decided natively from the global cursor position.
fn track_hover(window: &WebviewWindow, notch: &Notch) {
    let frame = ns_window(window).frame();
    let p = NSEvent::mouseLocation();
    let inside = p.x >= frame.origin.x
        && p.x <= frame.origin.x + frame.size.width
        && p.y >= frame.origin.y
        && p.y <= frame.origin.y + frame.size.height;
    if inside != EXPANDED_NOW.swap(inside, Ordering::Relaxed) {
        place(window, notch, inside);
        let _ = window.emit("hover", inside);
    }
}

fn poll_sessions(window: &WebviewWindow, notch: &'static Notch) {
    let now = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs());
    let summary = summarize(&read_dir(&sessions_dir()), now);
    {
        let mut latest = LATEST.lock().unwrap_or_else(|e| e.into_inner());
        if latest.as_ref() == Some(&summary) {
            return;
        }
        ROW_COUNT.store(summary.rows.len(), Ordering::Relaxed);
        *latest = Some(summary.clone());
    }
    let _ = window.emit("sessions", &summary);
    let target = window.clone();
    let _ = window.run_on_main_thread(move || {
        if EXPANDED_NOW.load(Ordering::Relaxed) {
            place(&target, notch, true);
        }
    });
}

#[tauri::command]
fn sessions() -> Option<Summary> {
    LATEST.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![sessions])
        .setup(|app| {
            let mtm = MainThreadMarker::new().expect("main thread");
            let Some(notch) = notched_screen(mtm) else {
                eprintln!("notch-spike: no notched display");
                std::process::exit(0);
            };
            let notch: &'static Notch = NOTCH.get_or_init(|| notch);
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);
            let window = WebviewWindowBuilder::new(app, "notch", WebviewUrl::App("index.html".into()))
                .decorations(false)
                .transparent(true)
                .resizable(false)
                .skip_taskbar(true)
                .visible(false)
                .build()?;
            let ns = ns_window(&window);
            ns.setLevel(ABOVE_MENU_BAR);
            ns.setCollectionBehavior(
                NSWindowCollectionBehavior::CanJoinAllSpaces
                    | NSWindowCollectionBehavior::Stationary
                    | NSWindowCollectionBehavior::FullScreenAuxiliary
                    | NSWindowCollectionBehavior::IgnoresCycle,
            );
            ns.setHasShadow(false);
            place(&window, notch, false);
            window.show()?;

            let hover = window.clone();
            std::thread::spawn(move || loop {
                std::thread::sleep(Duration::from_millis(50));
                let target = hover.clone();
                let _ = hover.run_on_main_thread(move || track_hover(&target, notch));
            });
            let poll = app.get_webview_window("notch").expect("notch window");
            std::thread::spawn(move || loop {
                poll_sessions(&poll, notch);
                std::thread::sleep(Duration::from_secs(1));
            });
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("tauri run");
}
