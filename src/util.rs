use std::{
    ffi::{CStr, OsStr, c_int},
    os::unix::ffi::OsStrExt,
    path::PathBuf,
    ptr::{self, NonNull},
};

use libc::pid_t;
use objc2::rc::Retained;
use objc2_app_kit::{NSRunningApplication, NSWorkspace, NSWorkspaceApplicationKey};
use objc2_application_services::{
    AXIsProcessTrustedWithOptions, AXUIElement, kAXTrustedCheckOptionPrompt,
};
use objc2_core_foundation::{CFBoolean, CFDictionary, CFRetained, CFString, CFType};
use objc2_foundation::{NSNotification, NSString};
use tracing::debug;

use crate::error::AccessibilityError;

// HACK: `kAX*` strings are not exported by `objc2-application-services`, so we
// manually write them out here.
pub const AX_APPLICATION_HIDDEN_NOTIFICATION: &str = "AXApplicationHidden";
const AX_FOCUSED_APPLICATION_ATTRIBUTE: &str = "AXFocusedApplication";
pub const AX_FOCUSED_WINDOW_CHANGED_NOTIFICATION: &str = "AXFocusedWindowChanged";

/// Returns the path of the current executable.
#[must_use]
pub fn exe_path() -> Option<PathBuf> {
    #[link(name = "Foundation", kind = "framework")]
    unsafe extern "C" {
        fn _NSGetExecutablePath(buf: *mut u8, buf_size: *mut u32) -> c_int;
    }

    let mut path_buf = [0_u8; 4096];

    #[allow(clippy::cast_possible_truncation)]
    let mut path_buf_size = path_buf.len() as u32;
    #[allow(clippy::used_underscore_items)]
    let path = unsafe { _NSGetExecutablePath(path_buf.as_mut_ptr(), &raw mut path_buf_size) == 0 }
        .then(|| CStr::from_bytes_until_nul(&path_buf).ok())??;
    Some(OsStr::from_bytes(path.to_bytes()).into())
}

/// Returns if the right privileges have been granted to use the
/// Accessibility APIs.
// https://github.com/koekeishiya/yabai/blob/a8eb6b1a7da4e33954b716b424eb51ce47317865/src/misc/helpers.h#L328
#[must_use]
pub fn has_ax_privileges() -> bool {
    let opts = CFDictionary::from_slices(
        &[unsafe { kAXTrustedCheckOptionPrompt }],
        &[CFBoolean::new(true)],
    );
    unsafe { AXIsProcessTrustedWithOptions(Some(opts.as_opaque())) }
}

fn ax_ui_element_value(
    elem: &AXUIElement,
    key: &str,
) -> Result<CFRetained<CFType>, AccessibilityError> {
    let mut val = ptr::null();
    AccessibilityError::wrap(unsafe {
        elem.copy_attribute_value(&CFString::from_str(key), NonNull::from(&mut val))
    })?;
    NonNull::new(val.cast_mut())
        .map(|val| unsafe { CFRetained::from_raw(val) })
        .ok_or(AccessibilityError::NoValue)
}

/// Converts a running application's PID to its Bundle ID.
#[must_use]
pub fn bundle_id_from_pid(pid: pid_t) -> Option<Retained<NSString>> {
    NSWorkspace::sharedWorkspace()
        .runningApplications()
        .iter()
        .find_map(|app| (app.processIdentifier() == pid).then(|| app.bundleIdentifier())?)
}

/// Returns the PID of the frontmost application from a notification
/// sent by `NotificationCenter`.
///
/// # Note
/// This function could always return `None` for certain notification types.
pub fn bundle_id_from_notification(notif: &NSNotification) -> Option<Retained<NSString>> {
    unsafe {
        Retained::cast_unchecked::<NSRunningApplication>(
            notif.userInfo()?.objectForKey(NSWorkspaceApplicationKey)?,
        )
        .bundleIdentifier()
    }
}

/// Returns the PID of the frontmost application from the Accessibility APIs.
pub fn pid_from_current_app() -> Result<pid_t, AccessibilityError> {
    let system_wide = unsafe { AXUIElement::new_system_wide() };
    let curr = ax_ui_element_value(&system_wide, AX_FOCUSED_APPLICATION_ATTRIBUTE)?
        .downcast::<AXUIElement>()
        .map_err(|_| AccessibilityError::NoValue)?;
    let mut pid = 0;
    AccessibilityError::wrap(unsafe { curr.pid(NonNull::from(&mut pid)) })?;
    Ok(pid)
}

/// Returns the Bundle ID of the frontmost application as indicated by
/// `NSWorkspace`.
///
/// # Note
/// Floating panels (such as the Spotlight search box triggered by cmd-space)
/// are ignored by this API.
#[must_use]
pub fn bundle_id_from_frontmost_app() -> Option<Retained<NSString>> {
    NSWorkspace::sharedWorkspace()
        .frontmostApplication()?
        .bundleIdentifier()
}

/// Returns the Bundle ID of the currently focused application.
///
/// # Note
/// This function always tries to get the current application from the
/// Accessibility APIs first, and uses the `NSWorkspace` result as a fallback.
/// Despite these efforts, the result might still be inaccurate.
#[must_use]
pub fn bundle_id_from_current_app() -> Option<Retained<NSString>> {
    match pid_from_current_app() {
        Ok(pid) => bundle_id_from_pid(pid),
        Err(e) => {
            debug!("failed to get current app PID, falling back to frontmost app PID: {e:?}");
            // HACK: I don't know why I am doing this, but this seems to work
            // 90% of the time.
            // TODO: What happens when `getCurrentAppPID()` fails and we fall
            // back to this one, but the frontmost app is NOT the current app?
            bundle_id_from_frontmost_app()
        }
    }
}
