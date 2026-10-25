//! The app's one supervisor of the remote control daemon. It follows the
//! `[remote]` settings with its own serial loader, brings the installed
//! service in line whenever they change, and asks the daemon for its status:
//! often while Settings or the pairing dialog shows it, rarely for the status
//! bar alone, and not at all while remote control is off. Every request and
//! service change runs on the background executor.

use super::service::{self, Outcome};
use crate::config::RemoteConfig;
use gpui::{App, BackgroundExecutor, Global, Task};
use herdr_remote_control::control::{self, Device, DeviceId, Request, Response, Status};
use std::{
    cell::Cell,
    path::{Path, PathBuf},
    rc::Rc,
    sync::Arc,
    time::{Duration, Instant},
};

/// How often the supervisor looks at what needs doing.
const TICK: Duration = Duration::from_millis(500);
/// How often the daemon is asked while something shows its details.
const WATCHED: Duration = Duration::from_secs(2);
/// How often it is asked for the title bar's indicator alone: often
/// enough that a phone connecting or leaving shows without a click.
const UNWATCHED: Duration = Duration::from_secs(3);

/// Sends one control request and waits for the answer. Blocking.
pub(crate) type Send = fn(&Path, &Request) -> Result<Response, control::Error>;
/// Brings the service in line with a config. Blocking.
type Reconcile = fn(&RemoteConfig, Send) -> crate::Result<Outcome>;
/// Restarts the service from the binary beside the app. Blocking.
type Restart = fn(&RemoteConfig) -> crate::Result<Outcome>;

/// What the daemon last said about itself.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) enum Health {
    /// Remote control is off.
    #[default]
    Off,
    /// On, and not asked yet.
    Unknown,
    Running(Status),
    /// On, but the daemon did not answer; why, for a person.
    NotRunning(String),
}

/// Keeps the daemon's details fresh while held: Settings' page and the
/// pairing dialog each hold one.
pub(crate) struct Lease(Rc<Cell<usize>>);

impl Drop for Lease {
    fn drop(&mut self) {
        self.0.set(self.0.get().saturating_sub(1));
    }
}

/// What a window needs to ask the daemon something itself.
#[derive(Clone)]
pub(crate) struct Client {
    address: PathBuf,
    send: Send,
    executor: BackgroundExecutor,
}

impl Client {
    /// Sends `request` on the background executor.
    pub(crate) fn request(&self, request: Request) -> Task<Result<Response, control::Error>> {
        let (address, send) = (self.address.clone(), self.send);
        self.executor.spawn(async move { send(&address, &request) })
    }

    /// Sends `request` and forgets the answer, as a closing dialog does.
    pub(crate) fn notify(&self, request: Request) {
        self.request(request).detach();
    }
}

pub(crate) struct Supervisor {
    // keep-sorted start
    /// Where the daemon listens; `None` without a state directory.
    address: Option<PathBuf>,
    /// The `[remote]` settings as last loaded.
    config: RemoteConfig,
    devices: Option<Vec<Device>>,
    /// Why the last service change failed, until the next one.
    failure: Option<Arc<crate::Error>>,
    health: Health,
    /// The status marks set in Herdr UI (`status_icons`), by config name,
    /// which the phone's lists draw too.
    indicators: Option<&'static str>,
    /// The set the running daemon last took.
    indicators_sent: Option<&'static str>,
    last_poll: Option<Instant>,
    /// Whether the settings were loaded once, before which nothing changes.
    loaded: bool,
    /// Ask now rather than at the next interval.
    poke: bool,
    polling: bool,
    quitting: bool,
    reconcile: Reconcile,
    reconciling: bool,
    restart: Restart,
    send: Send,
    /// The settings the service was last brought in line with.
    synced: Option<RemoteConfig>,
    watchers: Rc<Cell<usize>>,
    // keep-sorted end
}

impl Default for Supervisor {
    fn default() -> Self {
        Self {
            // keep-sorted start
            address: super::state_dir().ok().map(|dir| control::address(&dir)),
            config: RemoteConfig::default(),
            devices: None,
            failure: None,
            health: Health::Off,
            indicators: None,
            indicators_sent: None,
            last_poll: None,
            loaded: false,
            poke: false,
            polling: false,
            quitting: false,
            reconcile: reconcile_host,
            reconciling: false,
            restart: restart_host,
            send: control::request,
            synced: None,
            watchers: Rc::default(),
            // keep-sorted end
        }
    }
}

impl Global for Supervisor {}

/// The real service change: this user's host, the daemon's address.
fn reconcile_host(config: &RemoteConfig, send: Send) -> crate::Result<Outcome> {
    let state_dir = super::state_dir()?;
    let address = control::address(&state_dir);
    let host = super::host(state_dir)?;
    service::reconcile(
        config,
        &host,
        |request| send(&address, request),
        service::run,
    )
    .map_err(Into::into)
}

/// The real restart: this user's host.
fn restart_host(config: &RemoteConfig) -> crate::Result<Outcome> {
    let host = super::host(super::state_dir()?)?;
    service::restart(config, &host, service::run).map_err(Into::into)
}

impl Supervisor {
    /// Installs the app's supervisor. Native test modes never do, so they
    /// never touch a service.
    pub(crate) fn install(cx: &mut App) {
        cx.set_global(Self::default());
        Self::start(cx);
    }

    fn start(cx: &mut App) {
        crate::config::watch::follow(cx, "remote", |config| config.remote, Self::apply_config)
            .detach();
        crate::config::watch::follow(
            cx,
            "status icons",
            |config| config.status_icons.as_str(),
            Self::apply_indicators,
        )
        .detach();
        cx.on_app_quit(|cx| {
            let supervisor = cx.global_mut::<Self>();
            supervisor.quitting = true;
            let stop = supervisor.config.enabled && !supervisor.config.keep_running;
            let (address, send) = (supervisor.address.clone(), supervisor.send);
            cx.background_executor().spawn(async move {
                if let (true, Some(address)) = (stop, address)
                    && let Err(error) = send(&address, &Request::Shutdown)
                {
                    tracing::debug!(%error, "Remote control daemon did not take the shutdown");
                }
            })
        })
        .detach();
        cx.spawn(async move |cx| {
            loop {
                cx.background_executor().timer(TICK).await;
                cx.update(Self::tick);
            }
        })
        .detach();
    }

    fn apply_config(cx: &mut App, config: RemoteConfig) {
        let supervisor = cx.global_mut::<Self>();
        if supervisor.loaded && supervisor.config == config {
            return;
        }
        supervisor.loaded = true;
        supervisor.config = config;
        Self::tick(cx);
        cx.refresh_windows();
    }

    /// Takes the status marks set in Herdr UI, for the next poll to send.
    pub(super) fn apply_indicators(cx: &mut App, name: &'static str) {
        let supervisor = cx.global_mut::<Self>();
        if supervisor.indicators != Some(name) {
            supervisor.indicators = Some(name);
            supervisor.poke = true;
        }
    }

    /// Starts what is due: a service change for new settings, then a poll.
    pub(crate) fn tick(cx: &mut App) {
        let executor = cx.background_executor().clone();
        let supervisor = cx.global_mut::<Self>();
        if supervisor.quitting || !supervisor.loaded {
            return;
        }
        if !supervisor.reconciling && supervisor.synced.as_ref() != Some(&supervisor.config) {
            supervisor.reconciling = true;
            let (config, reconcile, send) = (
                supervisor.config.clone(),
                supervisor.reconcile,
                supervisor.send,
            );
            let task = executor.spawn(async move {
                let result = reconcile(&config, send);
                (config, result)
            });
            cx.spawn(async move |cx| {
                let (config, result) = task.await;
                cx.update(|cx| {
                    cx.global_mut::<Self>().reconciled(config, result);
                    cx.refresh_windows();
                });
            })
            .detach();
        }
        let supervisor = cx.global_mut::<Self>();
        if !supervisor.config.enabled {
            if supervisor.health != Health::Off || supervisor.devices.is_some() {
                supervisor.health = Health::Off;
                supervisor.devices = None;
                cx.refresh_windows();
            }
            return;
        }
        if supervisor.health == Health::Off {
            supervisor.health = Health::Unknown;
        }
        let watched = supervisor.watchers.get() > 0;
        let interval = if watched { WATCHED } else { UNWATCHED };
        let due = supervisor.poke
            || supervisor
                .last_poll
                .is_none_or(|last| last.elapsed() >= interval);
        let Some(address) = supervisor
            .address
            .clone()
            .filter(|_| due && !supervisor.polling)
        else {
            return;
        };
        supervisor.polling = true;
        supervisor.poke = false;
        supervisor.last_poll = Some(Instant::now());
        let send = supervisor.send;
        let indicators = supervisor
            .indicators
            .filter(|name| supervisor.indicators_sent != Some(*name));
        let task = executor.spawn(async move {
            let status = send(&address, &Request::Status);
            // Every poll: the title bar shows whether any phone is paired.
            let devices = status.is_ok().then(|| send(&address, &Request::Devices));
            // The daemon keeps the set across restarts; it is sent again
            // only when it changes or a poll found the daemon down.
            let sent = indicators.filter(|name| {
                status.is_ok()
                    && send(
                        &address,
                        &Request::Indicators {
                            set: (*name).to_owned(),
                        },
                    )
                    .is_ok()
            });
            (status, devices, sent)
        });
        cx.spawn(async move |cx| {
            let (status, devices, sent) = task.await;
            cx.update(|cx| {
                let supervisor = cx.global_mut::<Self>();
                if status.is_err() {
                    supervisor.indicators_sent = None;
                } else if sent.is_some() {
                    supervisor.indicators_sent = sent;
                }
                if supervisor.polled(status, devices) {
                    cx.refresh_windows();
                }
            });
        })
        .detach();
    }

    fn reconciled(&mut self, config: RemoteConfig, result: crate::Result<Outcome>) {
        self.reconciling = false;
        self.poke = true;
        match result {
            Ok(outcome) => {
                tracing::info!(?outcome, enabled = config.enabled, "Remote control service");
                self.failure = None;
            }
            Err(error) => {
                tracing::warn!(%error, "Cannot change the remote control service");
                self.failure = Some(Arc::new(error));
            }
        }
        // A failure is not retried on its own; changing a setting, or Retry
        // in Settings, tries again.
        self.synced = Some(config);
    }

    /// Takes a poll's answers. Returns whether anything shown changed.
    fn polled(
        &mut self,
        status: Result<Response, control::Error>,
        devices: Option<Result<Response, control::Error>>,
    ) -> bool {
        self.polling = false;
        if !self.config.enabled {
            return false;
        }
        let health = match status {
            Ok(Response::Status(status)) => Health::Running(status),
            Ok(_) => Health::NotRunning(super::Error::Unexpected.to_string()),
            Err(error) => Health::NotRunning(error.to_string()),
        };
        let devices = match devices {
            Some(Ok(Response::Devices { devices })) => Some(devices),
            // Not asked: the last list stands while the daemon runs.
            None if matches!(health, Health::Running(_)) => self.devices.clone(),
            _ => None,
        };
        let changed = health != self.health || devices != self.devices;
        self.health = health;
        self.devices = devices;
        changed
    }

    // Reading, for windows and Settings.

    pub(crate) fn health(cx: &App) -> Health {
        cx.try_global::<Self>()
            .map_or(Health::Off, |supervisor| supervisor.health.clone())
    }

    pub(crate) fn devices(cx: &App) -> Option<&[Device]> {
        cx.try_global::<Self>()?.devices.as_deref()
    }

    pub(crate) fn failure(cx: &App) -> Option<Arc<crate::Error>> {
        cx.try_global::<Self>()?.failure.clone()
    }

    /// Whether the service is being changed now.
    pub(crate) fn busy(cx: &App) -> bool {
        cx.try_global::<Self>()
            .is_some_and(|supervisor| supervisor.reconciling)
    }

    /// Phones with a stream open now, for the status bar.
    pub(crate) fn connected(cx: &App) -> usize {
        match cx.try_global::<Self>().map(|supervisor| &supervisor.health) {
            Some(Health::Running(status)) => status.connected,
            _ => 0,
        }
    }

    /// Keeps the details fresh until the lease is dropped, starting now.
    pub(crate) fn lease(cx: &mut App) -> Option<Lease> {
        if !cx.has_global::<Self>() {
            return None;
        }
        let supervisor = cx.global_mut::<Self>();
        supervisor.watchers.set(supervisor.watchers.get() + 1);
        supervisor.poke = true;
        Some(Lease(supervisor.watchers.clone()))
    }

    /// Asks the daemon again at the next tick.
    pub(crate) fn refresh(cx: &mut App) {
        if cx.has_global::<Self>() {
            cx.global_mut::<Self>().poke = true;
        }
    }

    /// Tries the last service change again, as after a failure.
    pub(crate) fn retry(cx: &mut App) {
        if !cx.has_global::<Self>() {
            return;
        }
        let supervisor = cx.global_mut::<Self>();
        supervisor.synced = None;
        supervisor.failure = None;
        Self::tick(cx);
    }

    /// Restarts the daemon from the binary beside the app, as after a
    /// rebuild or update. Returns false when it is off or already changing.
    pub(crate) fn restart(cx: &mut App) -> bool {
        let Some(supervisor) = cx.try_global::<Self>() else {
            return false;
        };
        if supervisor.quitting || supervisor.reconciling || !supervisor.config.enabled {
            return false;
        }
        let (config, restart) = (supervisor.config.clone(), supervisor.restart);
        cx.global_mut::<Self>().reconciling = true;
        let task = cx
            .background_executor()
            .spawn(async move { (restart(&config), config) });
        cx.spawn(async move |cx| {
            let (result, config) = task.await;
            cx.update(|cx| {
                cx.global_mut::<Self>().reconciled(config, result);
                cx.refresh_windows();
            });
        })
        .detach();
        cx.refresh_windows();
        true
    }

    pub(crate) fn client(cx: &App) -> Option<Client> {
        let supervisor = cx.try_global::<Self>()?;
        Some(Client {
            address: supervisor.address.clone()?,
            send: supervisor.send,
            executor: cx.background_executor().clone(),
        })
    }

    /// Revokes `device` and refreshes the list once the daemon has. `done`
    /// gets the outcome on the UI thread.
    pub(crate) fn revoke(
        cx: &mut App,
        device: DeviceId,
        done: impl FnOnce(Result<(), control::Error>, &mut App) + 'static,
    ) {
        let Some(client) = Self::client(cx) else {
            return;
        };
        let task = client.request(Request::Revoke { device });
        cx.spawn(async move |cx| {
            let result = task.await.map(drop);
            cx.update(|cx| {
                Self::refresh(cx);
                done(result, cx);
            });
        })
        .detach();
    }

    /// A supervisor whose service is in line with `config` and that asks
    /// `send`, as tests use.
    #[cfg(test)]
    pub(crate) fn fixture(cx: &mut App, config: RemoteConfig, send: Send) {
        cx.set_global(Self {
            address: Some(PathBuf::from("remote-control.sock")),
            synced: Some(config.clone()),
            config,
            loaded: true,
            reconcile: |_, _| Ok(Outcome::Unchanged),
            restart: |_| Ok(Outcome::Started),
            send,
            ..Self::default()
        });
    }

    /// Replaces the restart, as tests outside this module do.
    #[cfg(test)]
    pub(crate) fn set_restart_for_tests(&mut self, restart: Restart) {
        self.restart = restart;
    }

    /// Takes a status and device list as a poll would have.
    #[cfg(test)]
    pub(crate) fn set_health(cx: &mut App, health: Health, devices: Option<Vec<Device>>) {
        let supervisor = cx.global_mut::<Self>();
        supervisor.health = health;
        supervisor.devices = devices;
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests;
