// https://github.com/tasuren/window-observer-rs/blob/6981559652fdefe656926814f81464c5c23046d4/src/platform_impl/macos/mod.rs

use std::{
    borrow::Cow,
    ffi::c_void,
    fmt,
    pin::Pin,
    ptr::{self, NonNull},
};

use accessibility_sys::{
    AXObserverAddNotification, AXObserverCreate, AXObserverGetRunLoopSource, AXObserverRef,
    AXObserverRemoveNotification, AXUIElementCreateApplication, AXUIElementRef,
};
use core_foundation::{
    base::{CFRelease, TCFType, ToVoid},
    runloop,
    string::{CFString, CFStringRef},
};
use libc::pid_t;
use tracing::debug;

use crate::error::AccessibilityError;

pub type OnNotifFn = Box<dyn Fn(&WindowObserver, Cow<'_, str>)>;

// Special thanks to
// <https://stackoverflow.com/questions/36264038/cocoa-programmatically-detect-frontmost-floating-windows>
// for providing the basic methodological guidance for supporting
// Spotlight and co.

// https://juejin.cn/post/6919716600543182855
pub struct WindowObserver {
    pid: pid_t,
    elem: AXUIElementRef,
    raw: AXObserverRef,
    pub on_notif: OnNotifFn,
}

impl fmt::Debug for WindowObserver {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("WindowObserver")
            .field("pid", &self.pid)
            .field("elem", &self.elem)
            .field("raw", &self.raw)
            .field("on_notif", &"<fn>")
            .finish()
    }
}

unsafe impl ToVoid<Self> for WindowObserver {
    fn to_void(&self) -> *const c_void {
        ptr::from_ref(self).cast()
    }
}

impl WindowObserver {
    #[must_use]
    pub const fn pid(&self) -> pid_t {
        self.pid
    }

    pub fn try_new(pid: pid_t, on_notif: OnNotifFn) -> Result<Pin<Box<Self>>, AccessibilityError> {
        unsafe extern "C" fn callback(
            _: AXObserverRef,
            _: AXUIElementRef,
            notif: CFStringRef,
            refcon: *mut c_void,
        ) {
            let Some(self_) = NonNull::new(refcon.cast()) else {
                return;
            };
            let self_: &WindowObserver = unsafe { self_.as_ref() };
            let pid = self_.pid();
            let notif = unsafe { CFString::wrap_under_get_rule(notif) };
            debug!("received `{notif}` from PID {pid}");
            (self_.on_notif)(self_, Cow::from(&notif));
        }

        let mut raw = ptr::null_mut();
        unsafe {
            AccessibilityError::wrap(AXObserverCreate(pid, callback, &mut raw))?;
        }
        Ok(Box::pin(Self {
            pid,
            on_notif,
            raw,
            elem: unsafe { AXUIElementCreateApplication(pid) },
        }))
    }

    pub fn subscribe(mut self: Pin<&mut Self>, notif: &str) -> Result<(), AccessibilityError> {
        AccessibilityError::wrap(unsafe {
            AXObserverAddNotification(
                self.raw,
                self.elem,
                CFString::new(notif).to_void().cast(),
                (&raw mut *self).cast(),
            )
        })
    }

    pub fn unsubscribe(&self, notif: &str) -> Result<(), AccessibilityError> {
        AccessibilityError::wrap(unsafe {
            AXObserverRemoveNotification(self.raw, self.elem, CFString::new(notif).to_void().cast())
        })
    }

    pub fn start(&mut self) {
        unsafe {
            runloop::CFRunLoopAddSource(
                runloop::CFRunLoopGetCurrent(),
                AXObserverGetRunLoopSource(self.raw),
                runloop::kCFRunLoopDefaultMode,
            );
        };
    }

    pub fn stop(&self) {
        if self.raw.is_null() {
            return;
        }
        unsafe {
            runloop::CFRunLoopRemoveSource(
                runloop::CFRunLoopGetCurrent(),
                AXObserverGetRunLoopSource(self.raw),
                runloop::kCFRunLoopDefaultMode,
            );
        }
    }
}

impl Drop for WindowObserver {
    fn drop(&mut self) {
        self.stop();
        unsafe {
            CFRelease(self.raw.cast());
        }
    }
}
