// https://github.com/tasuren/window-observer-rs/blob/6981559652fdefe656926814f81464c5c23046d4/src/platform_impl/macos/mod.rs

use std::{
    borrow::Cow,
    ffi::c_void,
    fmt,
    pin::Pin,
    ptr::{self, NonNull},
};

use libc::pid_t;
use objc2_application_services::{AXObserver, AXUIElement};
use objc2_core_foundation::{CFRetained, CFRunLoop, CFString, kCFRunLoopDefaultMode};
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
    elem: CFRetained<AXUIElement>,
    raw: CFRetained<AXObserver>,
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

impl WindowObserver {
    #[must_use]
    pub const fn pid(&self) -> pid_t {
        self.pid
    }

    pub fn try_new(pid: pid_t, on_notif: OnNotifFn) -> Result<Pin<Box<Self>>, AccessibilityError> {
        unsafe extern "C-unwind" fn callback(
            _: NonNull<AXObserver>,
            _: NonNull<AXUIElement>,
            notif: NonNull<CFString>,
            refcon: *mut c_void,
        ) {
            let Some(self_) = NonNull::new(refcon.cast()) else {
                return;
            };
            let self_: &WindowObserver = unsafe { self_.as_ref() };
            let pid = self_.pid();
            let notif = unsafe { notif.as_ref() }.to_string();
            debug!("received `{notif}` from PID {pid}");
            (self_.on_notif)(self_, Cow::Owned(notif));
        }

        let mut raw = ptr::null_mut();
        AccessibilityError::wrap(unsafe {
            AXObserver::create(pid, Some(callback), NonNull::from(&mut raw))
        })?;
        let raw = NonNull::new(raw).ok_or(AccessibilityError::Failure)?;
        Ok(Box::pin(Self {
            pid,
            on_notif,
            raw: unsafe { CFRetained::from_raw(raw) },
            elem: unsafe { AXUIElement::new_application(pid) },
        }))
    }

    pub fn subscribe(mut self: Pin<&mut Self>, notif: &str) -> Result<(), AccessibilityError> {
        let refcon = (&raw mut *self).cast();
        AccessibilityError::wrap(unsafe {
            self.raw
                .add_notification(&self.elem, &CFString::from_str(notif), refcon)
        })
    }

    pub fn unsubscribe(&self, notif: &str) -> Result<(), AccessibilityError> {
        AccessibilityError::wrap(unsafe {
            self.raw
                .remove_notification(&self.elem, &CFString::from_str(notif))
        })
    }

    pub fn start(&mut self) {
        let Some(run_loop) = CFRunLoop::current() else {
            return;
        };
        run_loop.add_source(Some(&*unsafe { self.raw.run_loop_source() }), unsafe {
            kCFRunLoopDefaultMode
        });
    }

    pub fn stop(&self) {
        let Some(run_loop) = CFRunLoop::current() else {
            return;
        };
        run_loop.remove_source(Some(&*unsafe { self.raw.run_loop_source() }), unsafe {
            kCFRunLoopDefaultMode
        });
    }
}

impl Drop for WindowObserver {
    fn drop(&mut self) {
        self.stop();
    }
}
