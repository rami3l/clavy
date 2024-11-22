use std::{ptr::NonNull, sync::LazyLock};

use block2::StackBlock;
use objc2::rc::Retained;
use objc2_foundation::{NSNotification, NSNotificationCenter, NSNotificationName, NSObject};
use tracing::trace;

pub static LOCAL_NOTIFICATION_CENTER: LazyLock<Retained<NSNotificationCenter>> =
    LazyLock::new(|| unsafe { NSNotificationCenter::new() });

pub const FOCUSED_WINDOW_CHANGED_NOTIFICATION: &str = "ClavyFocusedWindowsChangedNotification";
pub const APP_HIDDEN_NOTIFICATION: &str = "ClavyAppHiddenNotification";

#[derive(Debug)]
pub struct NotificationObserver {
    center: Retained<NSNotificationCenter>,
    raw: Retained<NSObject>,
}

impl NotificationObserver {
    pub fn new(
        center: Retained<NSNotificationCenter>,
        name: &NSNotificationName,
        update: impl Fn(NonNull<NSNotification>) + Clone + 'static,
    ) -> Self {
        let raw = unsafe {
            center.addObserverForName_object_queue_usingBlock(
                Some(name),
                None,
                None,
                &StackBlock::new(move |notif: NonNull<NSNotification>| {
                    trace!("received `{}`", notif.as_ref().name());
                    update(notif);
                }),
            )
        };
        Self { center, raw }
    }
}

impl Drop for NotificationObserver {
    fn drop(&mut self) {
        unsafe { self.center.removeObserver(&self.raw) };
    }
}
