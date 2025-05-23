use std::{
    collections::{HashMap, HashSet},
    ffi::c_void,
    pin::Pin,
    ptr,
    sync::{Mutex, OnceLock},
};

use accessibility_sys::{kAXApplicationHiddenNotification, kAXFocusedWindowChangedNotification};
use core_foundation::{base::FromVoid, dictionary::CFDictionary, number::CFNumber};
use core_graphics::window::{
    copy_window_info, kCGNullWindowID, kCGWindowListOptionAll, kCGWindowOwnerPID,
};
use libc::pid_t;
use objc2::{
    AllocAnyThread, DeclaredClass, define_class, msg_send,
    rc::{Allocated, Retained},
    runtime::AnyObject,
};
use objc2_app_kit::{NSRunningApplication, NSWorkspace};
use objc2_foundation::{
    NSDictionary, NSKeyValueChangeKey, NSKeyValueObservingOptions, NSNotificationName, NSNumber,
    NSObject, NSObjectNSKeyValueObserverRegistration, NSString, ns_string,
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
    allowed_app_ids: OnceLock<HashSet<String>>,
}

define_class![
    #[unsafe(super = NSObject)]
    #[name = "WorkspaceObserver"]
    #[ivars = WorkspaceObserverIvars]
    pub struct WorkspaceObserver;

    impl WorkspaceObserver {
        #[unsafe(method_id(init))]
        fn init(this: Allocated<Self>) -> Option<Retained<Self>> {
            let this = this.set_ivars(WorkspaceObserverIvars {
                workspace: unsafe { NSWorkspace::sharedWorkspace() },
                children: Mutex::default(),
                allowed_app_ids: OnceLock::default(),
            });
            unsafe { msg_send![super(this), init] }
        }

        #[unsafe(method(observeValueForKeyPath:ofObject:change:context:))]
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

const RUNNING_APPLICATIONS: &str = "runningApplications";

impl WorkspaceObserver {
    /// Creating `AXObserver` for some system apps is simply impossible.
    const EXCLUDED_APP_IDS: [&str; 4] = [
        "com.apple.dock",
        "com.apple.universalcontrol",
        // HACK: When hiding some system apps, `AXApplicationHidden` is not sent.
        // We exclude these apps from the observation for now.
        // See: https://github.com/rami3l/clavy/issues/3
        "com.apple.controlcenter",
        "com.apple.notificationcenterui",
    ];
    /// A list of known Spotlight-like apps that only show a popup window.
    /// See: <https://github.com/runjuu/InputSourcePro/blob/3ce832f1fb3b96a8cd6619b5868d55be38c5ca9f/Input%20Source%20Pro/Utilities/AppKit/NSApplication.swift#L5-L23>
    const KNOWN_POPUP_ONLY_APP_IDS: [&str; 12] = [
        "com.apple.Spotlight",
        "com.runningwithcrayons.Alfred",
        "at.obdev.LaunchBar",
        "com.raycast.macos",
        "com.googlecode.iterm2",
        "com.xunyong.hapigo",
        "com.hezongyidev.Bob",
        "com.ripperhe.Bob",
        "org.yuanli.utools",
        "com.1password.1password",
        "com.eusoft.eudic.LightPeek",
        "com.contextsformac.Contexts",
    ];

    #[must_use]
    pub fn new<S: AsRef<str>>(allowed_app_ids: impl IntoIterator<Item = S>) -> Retained<Self> {
        let res: Retained<Self> = unsafe { msg_send![Self::alloc(), init] };
        let mut allowed_app_ids: HashSet<_> = allowed_app_ids
            .into_iter()
            .map(|s| s.as_ref().to_owned())
            .collect();
        for id in Self::KNOWN_POPUP_ONLY_APP_IDS {
            allowed_app_ids.insert(id.to_owned());
        }
        for id in Self::EXCLUDED_APP_IDS {
            allowed_app_ids.remove(id);
        }
        res.ivars().allowed_app_ids.set(allowed_app_ids).unwrap();
        res.start();
        res
    }

    fn start(&self) {
        unsafe {
            self.ivars()
                .workspace
                .addObserver_forKeyPath_options_context(
                    self,
                    ns_string!(RUNNING_APPLICATIONS),
                    NSKeyValueObservingOptions::Initial
                        | NSKeyValueObservingOptions::Old
                        | NSKeyValueObservingOptions::New,
                    ptr::null_mut(),
                );
        }
    }

    fn stop(&self) {
        unsafe {
            self.ivars().workspace.removeObserver_forKeyPath_context(
                self,
                ns_string!(RUNNING_APPLICATIONS),
                ptr::null_mut(),
            );
        }
    }

    fn update(
        &self,
        key_path: Option<&NSString>,
        _change: Option<&NSDictionary<NSKeyValueChangeKey, AnyObject>>,
    ) {
        if !key_path.is_some_and(|p| unsafe { p.isEqualToString(ns_string!(RUNNING_APPLICATIONS)) })
        {
            warn!("received an unexpected change from key path `{key_path:?}`");
            return;
        }

        let ivars = self.ivars();

        let new = unsafe { ivars.workspace.runningApplications() };
        let new_keys = self.window_change_pids(&new.to_vec());

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

    fn window_change_pids(
        &self,
        running_apps: &[Retained<NSRunningApplication>],
    ) -> HashSet<pid_t> {
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

        running_apps
            .iter()
            .filter(|&app| {
                unsafe { app.bundleIdentifier() }.is_some_and(|nss| {
                    self.ivars()
                        .allowed_app_ids
                        .get()
                        .unwrap()
                        .contains(&nss.to_string())
                })
            })
            .map(|app| unsafe { app.processIdentifier() })
            .filter(|pid| windowed_pids.contains(pid))
            .collect()
    }
}

impl Drop for WorkspaceObserver {
    fn drop(&mut self) {
        self.stop();
    }
}
