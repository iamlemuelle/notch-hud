use std::cell::{Cell, RefCell};

use objc2::rc::Retained;
use objc2::{define_class, msg_send, sel, MainThreadOnly};
use objc2_app_kit::{
    NSApplication, NSBackingStoreType, NSColor, NSPanel, NSResponder, NSWindow, NSWindowCollectionBehavior,
    NSWindowDidResignKeyNotification, NSWindowStyleMask,
};
use objc2_foundation::{MainThreadMarker, NSNotification, NSNotificationCenter, NSObject, NSRect};
use tauri::menu::{Menu, PredefinedMenuItem, Submenu};
use tauri::{AppHandle, Emitter, WebviewWindow};

use crate::{trace_on, APP};

// kCGMainMenuWindowLevel is 24; +3 matches what notch apps use to clear the menu bar and its status items.
const ABOVE_MENU_BAR: isize = 24 + 3;

thread_local! {
    /// The visible window: a native non-activating panel that hosts the Tauri window's webview. Created once on
    /// the main thread at startup and kept for the app's lifetime; only the main thread can reach it.
    pub(crate) static PANEL: RefCell<Option<Retained<NotchHudPanel>>> = const { RefCell::new(None) };
    /// Whether the panel may become key. Only `focus(true)` sets it; `focus(false)` and every resign clear it.
    static KEY_WANTED: Cell<bool> = const { Cell::new(false) };
}

define_class!(
    // SAFETY: NSPanel has no subclassing requirements beyond its designated initializer, and NotchHudPanel has no Drop.
    #[unsafe(super(NSPanel, NSWindow, NSResponder, NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "NotchHudPanel"]
    pub(crate) struct NotchHudPanel;

    impl NotchHudPanel {
        #[unsafe(method(canBecomeKeyWindow))]
        fn can_become_key_window(&self) -> bool {
            KEY_WANTED.get()
        }

        #[unsafe(method(didResignKey:))]
        fn did_resign_key(&self, _notification: &NSNotification) {
            KEY_WANTED.set(false);
            if trace_on() {
                eprintln!("key: off");
            }
            if let Some(app) = APP.get() {
                let _ = app.emit("key", false);
            }
        }
    }
);

impl NotchHudPanel {
    fn new(mtm: MainThreadMarker, frame: NSRect) -> Retained<NotchHudPanel> {
        let this = NotchHudPanel::alloc(mtm).set_ivars(());
        // SAFETY: NSPanel's designated initializer.
        unsafe {
            msg_send![
                super(this),
                initWithContentRect: frame,
                styleMask: PANEL_STYLE,
                backing: NSBackingStoreType::Buffered,
                defer: false
            ]
        }
    }
}

const PANEL_STYLE: NSWindowStyleMask =
    NSWindowStyleMask::Borderless.union(NSWindowStyleMask::NonactivatingPanel);

/// Main thread only: moves the webview of `window` into a new non-activating panel and shows the panel.
/// A plain window, or one turned into a panel afterwards, activates the app on click; this one does not.
/// The Tauri window stays hidden and must never be shown: it would activate the app on click.
pub(crate) fn adopt_panel(mtm: MainThreadMarker, window: &WebviewWindow) {
    let ptr = window.ns_window().expect("ns_window") as *const objc2_app_kit::NSWindow;
    // SAFETY: Tauri owns this window for the app's lifetime; we only read it here, on the main thread.
    let tao = unsafe { &*ptr };
    let panel = NotchHudPanel::new(mtm, tao.frame());
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
    // SAFETY: the panel implements `didResignKey:` and outlives the registration (PANEL).
    unsafe {
        NSNotificationCenter::defaultCenter().addObserver_selector_name_object(
            &panel,
            sel!(didResignKey:),
            Some(NSWindowDidResignKeyNotification),
            Some(&panel),
        );
    }
    panel.orderFrontRegardless();
    PANEL.with(|p| *p.borrow_mut() = Some(panel));
}

/// The app's menu: Edit items only (CONTRACT.md section 11). While the panel is key, menu key equivalents reach this
/// app, so the default menu's Quit, Hide and Close Window would act on notch-hud; Command-V and the other edit keys
/// need these items.
pub(crate) fn edit_menu(app: &AppHandle) -> tauri::Result<Menu<tauri::Wry>> {
    let edit = Submenu::with_items(
        app,
        "Edit",
        true,
        &[
            &PredefinedMenuItem::undo(app, None)?,
            &PredefinedMenuItem::redo(app, None)?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::cut(app, None)?,
            &PredefinedMenuItem::copy(app, None)?,
            &PredefinedMenuItem::paste(app, None)?,
            &PredefinedMenuItem::select_all(app, None)?,
        ],
    )?;
    Menu::with_items(app, &[&edit])
}

/// Gives the panel key focus so typing reaches the page, or gives it back (CONTRACT.md section 3, "Key focus").
/// Returns whether the panel is key afterwards (always `false` for off), so the page can tell a request that did not
/// take. Synchronous like `surface`, so it runs on the main thread in arrival order. Never calls `resignKeyWindow`:
/// the panel was key again 3 s after it in the spike.
#[tauri::command]
pub(crate) fn focus(on: bool) -> bool {
    let Some(mtm) = MainThreadMarker::new() else { return false };
    let Some(panel) = PANEL.with(|p| p.borrow().clone()) else { return false };
    if !on {
        KEY_WANTED.set(false);
        NSApplication::sharedApplication(mtm).deactivate();
        return false;
    }
    // A click on the panel after a give-back can make it key before this runs (measured), so `key: on` keys off the
    // flag, which is off whenever the keyboard was given back.
    let was_wanted = KEY_WANTED.replace(true);
    panel.makeKeyWindow();
    // wry's parent view holds one subview, the WKWebView.
    if let Some(webview) = panel.contentView().and_then(|view| view.subviews().firstObject()) {
        panel.makeFirstResponder(Some(&webview));
    }
    let key = panel.isKeyWindow();
    if !key {
        KEY_WANTED.set(false);
    } else if !was_wanted && trace_on() {
        eprintln!("key: on");
    }
    key
}
