use std::cell::RefCell;

use objc2::rc::Retained;
use objc2::MainThreadOnly;
use objc2_app_kit::{NSBackingStoreType, NSColor, NSPanel, NSWindowCollectionBehavior, NSWindowStyleMask};
use objc2_foundation::MainThreadMarker;
use tauri::WebviewWindow;

// kCGMainMenuWindowLevel is 24; +3 matches what notch apps use to clear the menu bar and its status items.
const ABOVE_MENU_BAR: isize = 24 + 3;

thread_local! {
    /// The visible window: a native non-activating panel that hosts the Tauri window's webview. Created once on
    /// the main thread at startup and kept for the app's lifetime; only the main thread can reach it.
    pub(crate) static PANEL: RefCell<Option<Retained<NSPanel>>> = const { RefCell::new(None) };
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
