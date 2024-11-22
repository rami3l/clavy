use std::{
    collections::{HashMap, HashSet},
    ffi::c_void,
    pin::Pin,
    ptr,
    sync::{LazyLock, Mutex},
};

use accessibility_sys::{kAXApplicationHiddenNotification, kAXFocusedWindowChangedNotification};
use core_foundation::{base::FromVoid, dictionary::CFDictionary, number::CFNumber};
use core_graphics::window::{
    copy_window_info, kCGNullWindowID, kCGWindowListOptionAll, kCGWindowOwnerPID,
};
use libc::pid_t;
use objc2::{
    declare_class, msg_send_id, mutability,
    rc::{Allocated, Retained},
    runtime::AnyObject,
    ClassType, DeclaredClass,
};
use objc2_app_kit::{NSRunningApplication, NSWorkspace};
use objc2_foundation::{
    ns_string, NSDictionary, NSKeyValueChangeKey, NSKeyValueObservingOptions, NSNotificationName,
    NSNumber, NSObject, NSObjectNSKeyValueObserverRegistration, NSString,
};
use tracing::{debug, trace, warn};

use super::window::WindowObserver;
use crate::observer::notification::{
    APP_HIDDEN_NOTIFICATION, FOCUSED_WINDOW_CHANGED_NOTIFICATION, LOCAL_NOTIFICATION_CENTER,
};

#[derive(Debug)]
pub struct WorkspaceObserverIvars {
    workspace: Retained<NSWorkspace>,
    children: Mutex<HashMap<pid_t, Pin<Box<WindowObserver>>>>,
}

declare_class![
    pub struct WorkspaceObserver;

    unsafe impl ClassType for WorkspaceObserver {
        type Super = NSObject;
        type Mutability = mutability::InteriorMutable;
        const NAME: &'static str = "WorkspaceObserver";
    }

    impl DeclaredClass for WorkspaceObserver {
        type Ivars = WorkspaceObserverIvars;
    }

    unsafe impl WorkspaceObserver {
        #[method_id(init)]
        fn init(this: Allocated<Self>) -> Option<Retained<Self>> {
            let this = this.set_ivars(WorkspaceObserverIvars {
                workspace: unsafe { NSWorkspace::sharedWorkspace() },
                children: Mutex::default(),
            });
            unsafe { msg_send_id![super(this), init] }
        }

        #[method(observeValueForKeyPath:ofObject:change:context:)]
        fn observe_value(
            &self,
            key_path: Option<&NSString>,
            _object: Option<&AnyObject>,
            change: Option<&NSDictionary<NSKeyValueChangeKey, AnyObject>>,
            _context: *mut c_void,
        ) {
            self.update(key_path, change);
        }
    }
];

pub static RUNNING_APPLICATIONS: LazyLock<&'static NSString> =
    LazyLock::new(|| ns_string!("runningApplications"));

impl WorkspaceObserver {
    #[must_use]
    pub fn new() -> Retained<Self> {
        let res: Retained<Self> = unsafe { msg_send_id![Self::alloc(), init] };
        res.start();
        res
    }

    fn start(&self) {
        unsafe {
            self.ivars()
                .workspace
                .addObserver_forKeyPath_options_context(
                    self,
                    *RUNNING_APPLICATIONS,
                    NSKeyValueObservingOptions::NSKeyValueObservingOptionInitial
                        | NSKeyValueObservingOptions::NSKeyValueObservingOptionOld
                        | NSKeyValueObservingOptions::NSKeyValueObservingOptionNew,
                    ptr::null_mut(),
                );
        }
    }

    fn stop(&self) {
        unsafe {
            self.ivars().workspace.removeObserver_forKeyPath_context(
                self,
                *RUNNING_APPLICATIONS,
                ptr::null_mut(),
            );
        }
    }

    fn update(
        &self,
        key_path: Option<&NSString>,
        _change: Option<&NSDictionary<NSKeyValueChangeKey, AnyObject>>,
    ) {
        if !key_path.is_some_and(|p| unsafe { p.isEqualToString(*RUNNING_APPLICATIONS) }) {
            warn!("received an unexpected change from key path `{key_path:?}`");
            return;
        }

        let ivars = self.ivars();

        let new = unsafe { ivars.workspace.runningApplications() };
        let new_keys = Self::window_change_pids(&new.to_vec_retained());

        let mut children = ivars.children.lock().expect("failed to lock children");
        let old_keys = children.keys().copied().collect::<HashSet<_>>();

        for pid in old_keys.difference(&new_keys) {
            trace!("removing from children: {pid}");
            children.remove(pid);
        }
        for pid in new_keys.difference(&old_keys) {
            trace!("adding to children: {pid}");
            _ = WindowObserver::try_new(
                *pid,
                Box::new(|obs, notif| {
                    #[allow(non_upper_case_globals)]
                    let name = match notif.as_ref() {
                        kAXFocusedWindowChangedNotification => FOCUSED_WINDOW_CHANGED_NOTIFICATION,
                        kAXApplicationHiddenNotification => APP_HIDDEN_NOTIFICATION,
                        notif => {
                            debug!("unexpected notification `{notif}` detected");
                            return;
                        }
                    };
                    unsafe {
                        LOCAL_NOTIFICATION_CENTER.postNotificationName_object(
                            &NSNotificationName::from_str(name),
                            Some(&NSNumber::new_i32(obs.pid())),
                        );
                    };
                }),
            )
            .and_then(|mut new| {
                new.as_mut()
                    .subscribe(kAXFocusedWindowChangedNotification)?;
                new.as_mut().subscribe(kAXApplicationHiddenNotification)?;
                new.start();
                children.insert(*pid, new);
                Ok(())
            })
            .inspect_err(|e| debug!("failed to create `WindowObserver` for PID {pid}: {e}"));
        }
        drop(children);
    }

    fn window_change_pids(running_apps: &[Retained<NSRunningApplication>]) -> HashSet<pid_t> {
        // https://apple.stackexchange.com/a/317705
        // https://gist.github.com/ljos/3040846
        // https://stackoverflow.com/a/61688877
        let window_info = copy_window_info(kCGWindowListOptionAll, kCGNullWindowID)
            .expect("failed to copy window info");

        let windowed_pids: HashSet<pid_t> = window_info
            .iter()
            .filter_map(|d| unsafe {
                let d = CFDictionary::from_void(*d);
                CFNumber::from_void(*d.find(kCGWindowOwnerPID)?).to_i32()
            })
            .collect();

        let excluded_app_ids = [
            // Creating `AXObserver` for some system apps is simply impossible.
            "com.apple.dock",
            "com.apple.universalcontrol",
            // HACK: When hiding some system apps, `AXApplicationHidden` is not sent.
            // We exclude these apps from the observation for now.
            // See: https://github.com/rami3l/Claveilleur/issues/3
            "com.apple.controlcenter",
            "com.apple.notificationcenterui",
        ]
        .map(NSString::from_str);

        running_apps
            .iter()
            .filter_map(|app| unsafe {
                app.bundleIdentifier()
                    .and_then(|nss| {
                        excluded_app_ids
                            .iter()
                            .find(|aid| aid.isEqualToString(&nss))
                    })
                    .is_none()
                    .then(|| app.processIdentifier())
            })
            .filter(|pid| windowed_pids.contains(pid))
            .collect()
    }
}

impl Drop for WorkspaceObserver {
    fn drop(&mut self) {
        self.stop();
    }
}
