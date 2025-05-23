use std::{env, str::FromStr};

use clap::{Parser, Subcommand, builder::FalseyValueParser};
use clavy::{
    error::{Error, Result},
    observer::{
        input_source::{
            InputSourceState, input_source, kTISNotifySelectedKeyboardInputSourceChanged,
            set_input_source,
        },
        notification::{
            APP_HIDDEN_NOTIFICATION, FOCUSED_WINDOW_CHANGED_NOTIFICATION,
            LOCAL_NOTIFICATION_CENTER, NotificationObserver,
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
use libc::pid_t;
use objc2::rc::Retained;
use objc2_app_kit::{NSWorkspace, NSWorkspaceDidActivateApplicationNotification};
use objc2_foundation::{NSDistributedNotificationCenter, NSNotification, NSNumber, NSString};
use smol::channel;
use tracing::{Level, debug, event, event_enabled, info, warn};

use crate::_built::GIT_VERSION;

// TODO: Replace this with `.unwrap_or()` when it's available in `const`.
const VERSION: &str = match GIT_VERSION {
    Some(v) => v,
    None => clap::crate_version!(),
};

/// The command line options to be collected.
#[derive(Clone, Debug, Parser)]
#[command(
    version = VERSION,
    author = clap::crate_authors!(),
    about = clap::crate_description!(),
    before_help = format!("{name} {VERSION}", name = clap::crate_name!()),
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
            warn!(
                "it looks like required accessibility privileges have not been granted yet, and the service might exit immediately on startup..."
            );
            warn!(
                "to fix this issue, you may need to update your configuration in `System Settings > Privacy & Security > Accessibility`"
            );
        }

        match self.subcmd.unwrap_or_default() {
            Subcmd::Launch => launch()?,
            Subcmd::Install => service()?.install()?,
            Subcmd::Uninstall => service()?.uninstall()?,
            Subcmd::Reinstall => service()?.reinstall()?,
            Subcmd::Start => service()?.start()?,
            Subcmd::Stop => service()?.stop()?,
            Subcmd::Restart => service()?.restart()?,
        }
        Ok(())
    }
}

#[allow(clippy::too_many_lines)]
fn launch() -> Result<()> {
    const NOTIF_NAME_LVL: Level = Level::DEBUG;
    let activation_signal = |notif: &NSNotification, bundle_id: Retained<NSString>| unsafe {
        (
            event_enabled!(NOTIF_NAME_LVL).then(|| notif.name().to_string()),
            bundle_id.to_string(),
        )
    };

    if !has_ax_privileges() {
        return Err(Error::AxPrivilegesNotDetected);
    }

    info!("Hello from clavy!");

    let input_source_state = InputSourceState::new();
    let (activation_tx, activation_rx) = channel::unbounded();
    let (input_source_tx, input_source_rx) = channel::unbounded();

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
                let pid: pid_t = Retained::cast_unchecked::<NSNumber>(pid).as_i32();
                let Some(bundle_id) = bundle_id_from_pid(pid) else {
                    return;
                };
                let tx = tx.clone();
                let signal = activation_signal(notif, bundle_id);
                smol::spawn(async move { tx.send(signal).await.unwrap() }).detach();
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
                let tx = tx.clone();
                let signal = activation_signal(notif, bundle_id);
                smol::spawn(async move { tx.send(signal).await.unwrap() }).detach();
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
                    let tx = tx.clone();
                    let signal = activation_signal(notif, bundle_id);
                    smol::spawn(async move { tx.send(signal).await.unwrap() }).detach();
                }
            },
        )
    };

    smol::spawn({
        let input_source_state = input_source_state.clone();
        async move {
            let mut prev_app = None;
            while let Ok((notif, curr_app)) = activation_rx.recv().await {
                if prev_app.as_ref() == Some(&curr_app) {
                    continue;
                }
                prev_app = Some(curr_app.clone());
                event!(
                    NOTIF_NAME_LVL,
                    "detected activation of app `{curr_app}` via `{notif}`",
                    // Unwrapping is safe here because we only send `Some()` with this level.
                    notif = notif.unwrap()
                );
                if let Some(old_src) = input_source_state.load(&curr_app) {
                    if set_input_source(&old_src) {
                        continue;
                    }
                }
                let new_src = input_source();
                debug!("registering input source for `{curr_app}` as `{new_src}`");
                input_source_state.save(curr_app, new_src);
            }
        }
    })
    .detach();

    let _curr_input_source_observer = unsafe {
        NotificationObserver::new(
            Retained::cast_unchecked(NSDistributedNotificationCenter::defaultCenter()),
            &*kTISNotifySelectedKeyboardInputSourceChanged.cast(),
            move |_| {
                smol::spawn({
                    let tx = input_source_tx.clone();
                    async move { tx.send(input_source()).await.unwrap() }
                })
                .detach();
            },
        )
    };

    smol::spawn(async move {
        let mut prev: Option<String> = None;
        while let Ok(src) = input_source_rx.recv().await {
            if prev.as_ref() == Some(&src) {
                continue;
            }
            prev = Some(src.clone());
            let Some(curr_app) = bundle_id_from_current_app() else {
                warn!("failed to get bundle ID from current app");
                continue;
            };
            debug!("updating input source for `{curr_app}` to `{src}`");
            input_source_state.save(curr_app.to_string(), src);
        }
    })
    .detach();

    unsafe { CFRunLoopRun() };
    Ok(())
}
