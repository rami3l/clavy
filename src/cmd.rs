use std::{env, str::FromStr, sync::mpsc};

use clap::{builder::FalseyValueParser, Parser, Subcommand};
use clavy::{
    error::{Error, Result},
    observer::{
        input_source::{
            input_source, kTISNotifySelectedKeyboardInputSourceChanged, set_input_source,
            InputSourceState,
        },
        notification::{
            NotificationObserver, APP_HIDDEN_NOTIFICATION, FOCUSED_WINDOW_CHANGED_NOTIFICATION,
            LOCAL_NOTIFICATION_CENTER,
        },
        workspace::WorkspaceObserver,
    },
    service::{self, Service},
    util::{
        bundle_id_from_current_app, bundle_id_from_notification, bundle_id_from_pid,
        has_ax_privileges,
    },
};
use core_foundation::runloop::CFRunLoopRun;
use dispatch2::{Queue, QueueAttribute};
use libc::pid_t;
use objc2::rc::Retained;
use objc2_app_kit::{NSWorkspace, NSWorkspaceDidActivateApplicationNotification};
use objc2_foundation::{NSDistributedNotificationCenter, NSNumber, NSString};
use tracing::{debug, info, warn, Level};

use crate::_built::GIT_VERSION;

fn version() -> &'static str {
    GIT_VERSION.unwrap_or(clap::crate_version!())
}

/// The command line options to be collected.
#[derive(Clone, Debug, Parser)]
#[command(
    version = version(),
    author = clap::crate_authors!(),
    about = clap::crate_description!(),
    before_help = format!("{} {}", clap::crate_name!(), version()),
)]
pub struct Clavy {
    #[clap(subcommand)]
    subcmd: Option<Subcmd>,

    /// Do not use colors in output.
    #[clap(long, env, value_parser = FalseyValueParser::new())]
    no_color: bool,
}

#[derive(Default, Copy, Clone, Debug, Subcommand)]
pub enum Subcmd {
    /// Launch the daemon directly in the console.
    #[default]
    Launch,

    /// Install the service.
    Install,

    /// Uninstall the service.
    Uninstall,

    /// Reinstall the service.
    Reinstall,

    /// Start the service.
    Start,

    /// Stop the service.
    Stop,

    /// Restart the service.
    Restart,
}

impl Clavy {
    pub(crate) fn dispatch(&self) -> Result<()> {
        fn service() -> Result<Service, Error> {
            Service::try_new(service::ID)
        }

        tracing_subscriber::fmt()
            .compact()
            .with_ansi(!self.no_color)
            .with_max_level(
                env::var_os("RUST_LOG")
                    .and_then(|s| Level::from_str(&s.to_string_lossy()).ok())
                    .unwrap_or(Level::INFO),
            )
            .init();

        if !has_ax_privileges() {
            warn!("it looks like required accessibility privileges have not been granted yet, and the service might exit immediately on startup...");
            warn!("to fix this issue, you may need to update your configuration in `System Settings > Privacy & Security > Accessibility`");
        }

        match self.subcmd.unwrap_or_default() {
            Subcmd::Launch => launch()?,
            Subcmd::Install => service()?.install()?,
            Subcmd::Uninstall => service()?.uninstall()?,
            Subcmd::Reinstall => service()?.reinstall()?,
            Subcmd::Start => service()?.start()?,
            Subcmd::Stop => service()?.stop()?,
            Subcmd::Restart => service()?.restart()?,
        };
        Ok(())
    }
}

#[allow(clippy::too_many_lines)]
fn launch() -> Result<()> {
    if !has_ax_privileges() {
        return Err(Error::AxPrivilegesNotDetected);
    }

    info!("Hello from clavy!");

    let input_source_state = InputSourceState::new();
    let (activation_tx, activation_rx) = mpsc::channel();
    let (input_source_tx, input_source_rx) = mpsc::channel();

    let queue = Queue::new(service::ID, QueueAttribute::Concurrent);

    let _workspace_observer = WorkspaceObserver::new();

    let _focused_window_observer = NotificationObserver::new(
        LOCAL_NOTIFICATION_CENTER.clone(),
        &NSString::from_str(FOCUSED_WINDOW_CHANGED_NOTIFICATION),
        {
            let tx = activation_tx.clone();
            move |notif| unsafe {
                let notif = notif.as_ref();
                let Some(pid) = notif.object() else {
                    return;
                };
                let pid: pid_t = Retained::cast::<NSNumber>(pid).as_i32();
                let Some(bundle_id) = bundle_id_from_pid(pid) else {
                    return;
                };
                tx.send((notif.name(), bundle_id.to_string())).unwrap();
            }
        },
    );

    let _app_hidden_observer = NotificationObserver::new(
        LOCAL_NOTIFICATION_CENTER.clone(),
        &NSString::from_str(APP_HIDDEN_NOTIFICATION),
        {
            let tx = activation_tx.clone();
            move |notif| unsafe {
                let notif = notif.as_ref();
                let Some(bundle_id) = bundle_id_from_current_app() else {
                    return;
                };
                tx.send((notif.name(), bundle_id.to_string())).unwrap();
            }
        },
    );

    let _did_activate_app_observer = unsafe {
        NotificationObserver::new(
            NSWorkspace::sharedWorkspace().notificationCenter(),
            NSWorkspaceDidActivateApplicationNotification,
            {
                let tx = activation_tx;
                move |notif| {
                    let notif = notif.as_ref();
                    let Some(bundle_id) = bundle_id_from_notification(notif) else {
                        return;
                    };
                    tx.send((notif.name(), bundle_id.to_string())).unwrap();
                }
            },
        )
    };

    queue.exec_async({
        let input_source_state = input_source_state.clone();
        move || {
            let mut prev_app = None;
            for (notif, curr_app) in activation_rx {
                if prev_app.as_ref() == Some(&curr_app) {
                    continue;
                }
                prev_app = Some(curr_app.clone());
                debug!("detected activation of app `{curr_app}` via `{notif}`");
                if let Some(old_src) = input_source_state.load(&curr_app) {
                    if set_input_source(&old_src) {
                        continue;
                    }
                }
                let new_src = input_source();
                debug!("registering input source for `{curr_app}` as `{new_src}`");
                input_source_state.save(curr_app.to_string(), new_src);
            }
        }
    });

    let _curr_input_source_observer = unsafe {
        NotificationObserver::new(
            Retained::cast(NSDistributedNotificationCenter::defaultCenter()),
            &*kTISNotifySelectedKeyboardInputSourceChanged.cast(),
            move |_| input_source_tx.send(input_source()).unwrap(),
        )
    };

    queue.exec_async(move || {
        let mut prev: Option<String> = None;
        for src in input_source_rx {
            if prev.as_ref() == Some(&src) {
                continue;
            }
            prev = Some(src.clone());
            let Some(curr_app) = bundle_id_from_current_app() else {
                warn!("failed to get bundle ID from current app");
                return;
            };
            debug!("updating input source for `{curr_app}` to `{src}`");
            input_source_state.save(curr_app.to_string(), src);
        }
    });

    unsafe { CFRunLoopRun() };
    Ok(())
}
