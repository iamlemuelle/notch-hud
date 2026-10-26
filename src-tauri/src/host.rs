//! Brings a session's app forward when its prompt box can't reach it. See CONTRACT.md section 11, "Opening the
//! session's app".

use objc2::rc::Retained;
use objc2_app_kit::{NSApplicationActivationOptions, NSRunningApplication};
use objc2_foundation::NSString;

use crate::prompt::HostOpen;

/// `NSRunningApplication` is thread safe, so neither function needs the main thread.
fn running(bundle_id: &str) -> Option<Retained<NSRunningApplication>> {
    NSRunningApplication::runningApplicationsWithBundleIdentifier(&NSString::from_str(bundle_id))
        .iter()
        .find(|app| !app.isTerminated())
}

fn name_of(app: &NSRunningApplication) -> Option<String> {
    app.localizedName().map(|n| n.to_string()).filter(|n| !n.is_empty())
}

pub fn app_name(bundle_id: &str) -> Option<String> {
    running(bundle_id).and_then(|app| name_of(&app))
}

/// Never launches: an app that isn't running is reported as such.
pub fn open(bundle_id: &str) -> Result<HostOpen, String> {
    let Some(app) = running(bundle_id) else { return Ok(HostOpen::NotRunning) };
    if !app.activateWithOptions(NSApplicationActivationOptions::empty()) {
        return Err("the activation request was refused".into());
    }
    Ok(HostOpen::Opened(name_of(&app).unwrap_or_else(|| bundle_id.to_string())))
}
