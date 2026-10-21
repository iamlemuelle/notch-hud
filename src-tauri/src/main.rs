use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use objc2_app_kit::{NSEvent, NSScreen, NSWindow, NSWindowCollectionBehavior};
use objc2_foundation::{MainThreadMarker, NSPoint, NSRect, NSSize};
use tauri::{Emitter, WebviewUrl, WebviewWindow, WebviewWindowBuilder};

// kCGMainMenuWindowLevel is 24; +3 matches what notch apps use to clear the menu bar and its status items.
const ABOVE_MENU_BAR: isize = 24 + 3;
const EAR: f64 = 60.0;
const EXPANDED: NSSize = NSSize { width: 420.0, height: 140.0 };

/// Notch geometry in AppKit points (bottom-left origin).
struct Notch {
    screen: NSRect,
    x: f64,
    width: f64,
    height: f64,
}

impl Notch {
    fn read(mtm: MainThreadMarker) -> Option<Notch> {
        let screen = NSScreen::mainScreen(mtm)?;
        let frame = screen.frame();
        let height = screen.safeAreaInsets().top;
        if height == 0.0 {
            return None;
        }
        let left = screen.auxiliaryTopLeftArea();
        let right = screen.auxiliaryTopRightArea();
        let x = left.origin.x + left.size.width;
        Some(Notch { screen: frame, x, width: right.origin.x - x, height })
    }

    fn top(&self) -> f64 {
        self.screen.origin.y + self.screen.size.height
    }

    fn panel(&self, expanded: bool) -> NSRect {
        let size = if expanded {
            EXPANDED
        } else {
            NSSize { width: self.width + 2.0 * EAR, height: self.height }
        };
        let center = self.x + self.width / 2.0;
        NSRect {
            origin: NSPoint { x: center - size.width / 2.0, y: self.top() - size.height },
            size,
        }
    }
}

fn ns_window(window: &WebviewWindow) -> &NSWindow {
    let ptr = window.ns_window().expect("ns_window") as *const NSWindow;
    unsafe { &*ptr }
}

fn place(window: &WebviewWindow, expanded: bool) {
    let mtm = MainThreadMarker::new().expect("main thread");
    let notch = Notch::read(mtm).expect("this display has no notch");
    let ns = ns_window(window);
    ns.setLevel(ABOVE_MENU_BAR);
    ns.setCollectionBehavior(
        NSWindowCollectionBehavior::CanJoinAllSpaces
            | NSWindowCollectionBehavior::Stationary
            | NSWindowCollectionBehavior::FullScreenAuxiliary
            | NSWindowCollectionBehavior::IgnoresCycle,
    );
    ns.setHasShadow(false);
    ns.setAcceptsMouseMovedEvents(true);
    ns.setFrame_display(notch.panel(expanded), true);
}

static EXPANDED_NOW: AtomicBool = AtomicBool::new(false);

// The webview gets no mouse events while this non-key window belongs to a background app,
// so hover is decided natively from the global cursor position.
fn track_hover(window: &WebviewWindow) {
    let frame = ns_window(window).frame();
    let p = NSEvent::mouseLocation();
    let inside = p.x >= frame.origin.x
        && p.x <= frame.origin.x + frame.size.width
        && p.y >= frame.origin.y
        && p.y <= frame.origin.y + frame.size.height;
    if inside != EXPANDED_NOW.swap(inside, Ordering::Relaxed) {
        place(window, inside);
        let _ = window.emit("hover", inside);
    }
}

fn main() {
    tauri::Builder::default()
        .setup(|app| {
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);
            let window = WebviewWindowBuilder::new(app, "notch", WebviewUrl::App("index.html".into()))
                .decorations(false)
                .transparent(true)
                .resizable(false)
                .shadow(false)
                .always_on_top(true)
                .skip_taskbar(true)
                .accept_first_mouse(true)
                .visible(false)
                .build()?;
            place(&window, false);
            window.show()?;
            std::thread::spawn(move || loop {
                std::thread::sleep(Duration::from_millis(50));
                let target = window.clone();
                let _ = window.run_on_main_thread(move || track_hover(&target));
            });
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("tauri run");
}
