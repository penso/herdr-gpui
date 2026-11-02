//! The activity bar's Automations section: the Luau scripts `herdr-automate`
//! runs on each host, on a schedule or by hand, and their runs.
//!
//! This app is only a client. It never runs a script: it runs
//! `herdr-automate COMMAND --json` on each machine the window reaches (this
//! one, SSH devices, cloud machines) through the same host scripts Teleport
//! uses, on the background executor, and draws the reports
//! (`herdr_automation::report`) it prints. A machine's status is read every
//! [`SHOWN`] while the section is on screen, and every [`HIDDEN`] otherwise
//! for the bar's count; Refresh reads at once.
//!
//! Anything that changes a host (approving a script, pausing, installing the
//! daemon) is a click in this section or a confirmation in the app's own
//! dialog; `herdr-gpui automation approve` only opens that dialog.
// i18n: migrated

mod actions;
mod clean;
mod dialog;
mod error;
mod fetch;
mod format;
mod hosts;
mod install;
mod list;
mod logs;
mod model;
mod notify;
mod page;
mod section;
mod timeline;

#[cfg(test)]
mod tests;

pub(crate) use actions::Action;
pub(crate) use dialog::Dialog;
pub use error::Error;
pub(crate) use hosts::{Machine, Wanted};
pub(crate) use logs::{LogSource, LogSources};
pub(crate) use model::Look;

use crate::{HerdrWindow, sidebar::ActivityPanel, teleport::Host};
use gpui::{prelude::*, *};
use herdr_automation::report::{LogText, RunDetail, Status};
use std::{
    collections::HashMap,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant, SystemTime},
};

/// How often a machine is read while the section is shown.
const SHOWN: Duration = Duration::from_secs(15);
/// How often otherwise, for the bar's count and notifications.
const HIDDEN: Duration = Duration::from_secs(60);
/// How often a host whose daemon will not stay up is started again.
pub(crate) const AUTOSTART_EVERY: Duration = Duration::from_secs(300);
/// Run pages and logs kept at once; past it, all but the open one go.
const MAX_PAGES: usize = 32;

/// What the last read of a machine found.
#[derive(Clone, Debug, Default)]
pub(crate) enum Read {
    #[default]
    Unread,
    Ready(Arc<Status>),
    /// The machine has no `herdr-automate`.
    Missing,
    /// The read failed; the last good status, if any, is still shown.
    Failed {
        error: Arc<Error>,
        last: Option<Arc<Status>>,
    },
}

impl Read {
    pub(crate) fn status(&self) -> Option<&Status> {
        match self {
            Self::Ready(status)
            | Self::Failed {
                last: Some(status), ..
            } => Some(status),
            Self::Unread | Self::Missing | Self::Failed { last: None, .. } => None,
        }
    }
}

/// One machine's place in the section.
pub(crate) struct HostState {
    pub(crate) machine: Machine,
    pub(crate) label: String,
    pub(crate) endpoint: Option<String>,
    pub(crate) host: Host,
    pub(crate) read: Read,
    pub(crate) read_at: Option<SystemTime>,
    /// When the last read started, which the next one is timed from.
    last: Option<Instant>,
    task: Option<Task<()>>,
    /// What a click asked of the host and is still doing.
    pub(crate) busy: Option<(Action, Task<()>)>,
    /// When the daemon was last started because scripts were listed with
    /// no daemon running, so a failing start is not retried every read.
    autostarted: Option<Instant>,
    /// What notifications already told about this host.
    seen: notify::Seen,
}

impl HostState {
    fn new(wanted: Wanted) -> Self {
        Self {
            machine: wanted.machine,
            label: wanted.label,
            endpoint: wanted.endpoint,
            host: wanted.host,
            read: Read::Unread,
            read_at: None,
            last: None,
            task: None,
            busy: None,
            autostarted: None,
            seen: notify::Seen::default(),
        }
    }

    pub(crate) fn status(&self) -> Option<&Status> {
        self.read.status()
    }

    pub(crate) fn reading(&self) -> bool {
        self.task.is_some()
    }

    /// Whether to start the host's daemon now: it lists scripts but none is
    /// running, nothing else is under way, and no automatic start was tried
    /// in the last [`AUTOSTART_EVERY`].
    pub(crate) fn wants_daemon(&self, now: Instant) -> bool {
        let Some(status) = self.status() else {
            return false;
        };
        !status.daemon.running
            && !status.scripts.is_empty()
            && self.busy.is_none()
            && self
                .autostarted
                .is_none_or(|at| now.saturating_duration_since(at) >= AUTOSTART_EVERY)
    }

    /// Whether a read is due. A machine without the program is not read
    /// on the timer: only when asked (Refresh, the section shown, Enable),
    /// which clears `last`.
    fn due(&self, now: Instant, every: Duration) -> bool {
        let timed = !matches!(self.read, Read::Missing);
        self.task.is_none()
            && match self.last {
                None => true,
                Some(last) => timed && now >= last + every,
            }
    }
}

/// Which overview the section shows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Mode {
    #[default]
    Hosts,
    Timeline,
}

impl Mode {
    /// Its name in the header's toggle and the page's trail.
    pub(crate) fn title(self) -> String {
        match self {
            Self::Hosts => crate::i18n::fl!("automations-mode-hosts"),
            Self::Timeline => crate::i18n::fl!("automations-mode-timeline"),
        }
    }

    /// The toggle's ID, the same in every language.
    fn key(self) -> &'static str {
        match self {
            Self::Hosts => "hosts",
            Self::Timeline => "timeline",
        }
    }
}

/// The script or run shown beside the list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Opened {
    Script {
        machine: Machine,
        path: String,
    },
    Run {
        machine: Machine,
        path: String,
        run: i64,
    },
}

impl Opened {
    pub(crate) fn machine(&self) -> &Machine {
        match self {
            Self::Script { machine, .. } | Self::Run { machine, .. } => machine,
        }
    }

    pub(crate) fn path(&self) -> &str {
        match self {
            Self::Script { path, .. } | Self::Run { path, .. } => path,
        }
    }
}

/// A run's page or a log, being read or read.
pub(crate) enum Loaded<T> {
    Reading { _task: Task<()> },
    Read(Arc<T>),
    Failed(Arc<Error>),
}

/// A log the run page shows in full: the run's, or one step's.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct LogKey {
    pub(crate) machine: Machine,
    pub(crate) run: i64,
    pub(crate) step: Option<u32>,
}

/// The section's state in a window.
#[derive(Default)]
pub(crate) struct Automations {
    pub(crate) hosts: Vec<HostState>,
    pub(crate) mode: Mode,
    pub(crate) opened: Option<Opened>,
    /// Whether the open page takes the whole section, the list folded away.
    pub(crate) list_hidden: bool,
    /// The one machine the timeline shows, or all.
    pub(crate) filter: Option<Machine>,
    pub(crate) runs: HashMap<(Machine, i64), Loaded<RunDetail>>,
    pub(crate) logs: HashMap<LogKey, Loaded<LogText>>,
    /// The version of the `herdr-automate` this app ships, once read.
    pub(crate) local_version: Option<String>,
    local_version_task: Option<Task<()>>,
    pub(crate) dialog: Option<Dialog>,
    /// Whether the section was on screen at the last tick, to read again
    /// what is not read on the timer when it comes back.
    shown: bool,
    /// Tells reads in flight to stop when the window goes.
    cancelled: Arc<AtomicBool>,
    pub(crate) scroll: ScrollHandle,
    pub(crate) page_scroll: ScrollHandle,
}

impl Drop for Automations {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }
}

impl Automations {
    pub(crate) fn host(&self, machine: &Machine) -> Option<&HostState> {
        self.hosts.iter().find(|host| host.machine == *machine)
    }

    fn host_mut(&mut self, machine: &Machine) -> Option<&mut HostState> {
        self.hosts.iter_mut().find(|host| host.machine == *machine)
    }

    pub(crate) fn cancelled(&self) -> Arc<AtomicBool> {
        self.cancelled.clone()
    }

    /// Scripts that wait on the user across machines: the bar's count.
    pub(crate) fn needing(&self) -> usize {
        self.hosts
            .iter()
            .filter_map(HostState::status)
            .flat_map(|status| &status.scripts)
            .filter(|script| model::needs_you(script))
            .count()
    }

    /// When the newest failed run the count holds ended.
    pub(crate) fn needing_since(&self) -> Option<chrono::DateTime<chrono::Utc>> {
        self.hosts
            .iter()
            .filter_map(HostState::status)
            .flat_map(|status| &status.scripts)
            .filter_map(|script| script.last_run.as_ref())
            .filter(|run| model::failed(run.state))
            .filter_map(|run| run.finished_at.or(Some(run.started_at)))
            .max()
            .and_then(|at| chrono::DateTime::from_timestamp(at, 0))
    }

    /// Runs going on now, across machines.
    pub(crate) fn running(&self) -> usize {
        self.hosts
            .iter()
            .filter_map(HostState::status)
            .flat_map(|status| &status.scripts)
            .filter(|script| script.running.is_some())
            .count()
    }

    /// Whether any machine is being read, for the Refresh button.
    pub(crate) fn busy(&self) -> bool {
        self.hosts.iter().any(HostState::reading)
    }

    /// Reads the machines without the program again at the next tick.
    fn recheck_missing(&mut self) {
        for host in &mut self.hosts {
            if matches!(host.read, Read::Missing) {
                host.last = None;
            }
        }
    }

    /// Reads every machine at the next tick.
    pub(crate) fn refresh(&mut self) {
        for host in &mut self.hosts {
            host.last = None;
        }
    }

    /// Keeps the machines the window reaches now, in its order, keeping
    /// what was read of those it already had.
    fn sync_hosts(&mut self, wanted: Vec<Wanted>) -> bool {
        let same = wanted.len() == self.hosts.len()
            && wanted.iter().zip(&self.hosts).all(|(wanted, host)| {
                wanted.machine == host.machine
                    && wanted.label == host.label
                    && wanted.endpoint == host.endpoint
            });
        if same {
            return false;
        }
        let mut old = std::mem::take(&mut self.hosts);
        self.hosts = wanted
            .into_iter()
            .map(
                |wanted| match old.iter().position(|host| host.machine == wanted.machine) {
                    Some(index) => {
                        let mut host = old.swap_remove(index);
                        host.label = wanted.label;
                        host.endpoint = wanted.endpoint;
                        host.host = wanted.host;
                        host
                    }
                    None => HostState::new(wanted),
                },
            )
            .collect();
        if self
            .filter
            .as_ref()
            .is_some_and(|filter| self.host(filter).is_none())
        {
            self.filter = None;
        }
        if self
            .opened
            .as_ref()
            .is_some_and(|opened| self.host(opened.machine()).is_none())
        {
            self.opened = None;
        }
        true
    }

    /// Installs a read of `machine`; what the notifications should say.
    fn install(
        &mut self,
        machine: &Machine,
        result: Result<Status, Error>,
        at: SystemTime,
    ) -> Vec<notify::Notice> {
        let Some(host) = self.host_mut(machine) else {
            return Vec::new();
        };
        host.task = None;
        host.read_at = Some(at);
        host.read = match result {
            Ok(status) => Read::Ready(Arc::new(status)),
            Err(Error::Missing) => Read::Missing,
            Err(error) => Read::Failed {
                error: Arc::new(error),
                last: host.read.status().cloned().map(Arc::new),
            },
        };
        let notices = match &host.read {
            Read::Ready(status) => host.seen.observe(&host.label, status),
            _ => Vec::new(),
        };
        self.keep_opened();
        notices
    }

    /// Installs a status for tests outside the module, as a read would.
    #[cfg(test)]
    pub(crate) fn install_for_test(&mut self, wanted: Wanted, read: Read) {
        let machine = wanted.machine.clone();
        if self.host(&machine).is_none() {
            self.hosts.push(HostState::new(wanted));
        }
        if let Some(host) = self.host_mut(&machine) {
            host.read = read;
            host.read_at = Some(SystemTime::now());
            host.last = Some(Instant::now());
        }
    }

    /// Closes a page whose script the host no longer lists.
    fn keep_opened(&mut self) {
        let Some(opened) = &self.opened else {
            return;
        };
        let Some(host) = self.host(opened.machine()) else {
            self.opened = None;
            return;
        };
        let listed = match host.read.status() {
            Some(status) => status
                .scripts
                .iter()
                .any(|script| script.path == opened.path()),
            // Not read, or not readable now: keep the page until it says.
            None => true,
        };
        if !listed {
            self.opened = None;
        }
    }

    /// Drops kept pages and logs past the bound, all but the open run's.
    fn trim(&mut self) {
        let open = match &self.opened {
            Some(Opened::Run { machine, run, .. }) => Some((machine.clone(), *run)),
            _ => None,
        };
        if self.runs.len() >= MAX_PAGES {
            self.runs.retain(|key, _| Some(key) == open.as_ref());
        }
        if self.logs.len() >= MAX_PAGES {
            self.logs
                .retain(|key, _| open.as_ref() == Some(&(key.machine.clone(), key.run)));
        }
    }
}

impl HerdrWindow {
    /// Runs every tick: keeps the machine list current and reads each one
    /// that is due.
    pub(crate) fn poll_automations(&mut self, cx: &mut Context<Self>) {
        // Read with the bar off too: notifications and `automation approve`
        // need the hosts' state whether or not the section can be shown.
        // Turned off in the settings, nothing is read or started.
        self.forget_closed_automation_dialog();
        if !self.activity_panel_listed(ActivityPanel::Automations) {
            LogSources::turn_off(cx);
            self.automations.shown = false;
            if self.automations.sync_hosts(Vec::new()) {
                cx.notify();
            }
            return;
        }
        LogSources::turn_on(cx);
        let wanted = self.automation_hosts();
        if self.automations.sync_hosts(wanted) {
            cx.notify();
        }
        self.read_local_automate_version(cx);
        let shown = self.activity_section() == Some(ActivityPanel::Automations);
        if shown && !self.automations.shown {
            self.automations.recheck_missing();
        }
        self.automations.shown = shown;
        let every = if shown { SHOWN } else { HIDDEN };
        let now = Instant::now();
        let due: Vec<Machine> = self
            .automations
            .hosts
            .iter()
            .filter(|host| host.due(now, every))
            .map(|host| host.machine.clone())
            .collect();
        for machine in due {
            self.read_automation_host(&machine, cx);
        }
    }

    fn read_automation_host(&mut self, machine: &Machine, cx: &mut Context<Self>) {
        let cancelled = self.automations.cancelled();
        let Some(host) = self.automations.host_mut(machine) else {
            return;
        };
        host.last = Some(Instant::now());
        let script_host = host.host.clone();
        let work = cx
            .background_executor()
            .spawn(async move { fetch::status(&script_host, &cancelled) });
        let machine = machine.clone();
        host.task = Some(cx.spawn(async move |this, cx| {
            let result = work.await;
            let _ = this.update(cx, |this, cx| {
                this.install_automation_status(&machine, result, cx);
            });
        }));
        cx.notify();
    }

    /// What a read found, and what follows from it: notifications, the log
    /// window's sources, and the open run's page read again while it runs.
    fn install_automation_status(
        &mut self,
        machine: &Machine,
        result: Result<Status, Error>,
        cx: &mut Context<Self>,
    ) {
        let notices = self.automations.install(machine, result, SystemTime::now());
        // Scripts are listed but nothing runs them: start the daemon, as
        // the user would with Start daemon.
        let now = Instant::now();
        if let Some(host) = self.automations.host_mut(machine)
            && host.wants_daemon(now)
        {
            host.autostarted = Some(now);
            self.automation_action(machine, Action::Start, cx);
        }
        if let Some(host) = self.automations.host(machine) {
            LogSources::observe(host, cx);
        }
        if let Some(Opened::Run {
            machine: open, run, ..
        }) = self.automations.opened.clone()
            && open == *machine
            && self.automation_run_live(&open, run)
        {
            self.read_automation_run(&open, run, cx);
        }
        self.post_automation_notices(notices, cx);
        cx.notify();
    }

    /// Whether `run`'s page may have gone stale since it was read, so it
    /// is read again with every status read (`model::run_stale`).
    fn automation_run_live(&self, machine: &Machine, run: i64) -> bool {
        let summary = self
            .automations
            .host(machine)
            .and_then(HostState::status)
            .and_then(|status| model::find_run(status, run));
        let detail = match self.automations.runs.get(&(machine.clone(), run)) {
            Some(Loaded::Read(detail)) => Some(&detail.run),
            _ => None,
        };
        model::run_stale(summary, detail, format::now())
    }

    /// Reads the version of the `herdr-automate` this app would install,
    /// once per window.
    fn read_local_automate_version(&mut self, cx: &mut Context<Self>) {
        if self.automations.local_version.is_some() || self.automations.local_version_task.is_some()
        {
            return;
        }
        let Ok(host) = Host::new(&herdr_client::ConnectTarget::Local) else {
            return;
        };
        let cancelled = self.automations.cancelled();
        let work = cx
            .background_executor()
            .spawn(async move { fetch::version(&host, &cancelled) });
        self.automations.local_version_task = Some(cx.spawn(async move |this, cx| {
            let version = work.await;
            let _ = this.update(cx, |this, cx| {
                // A missing local binary is not retried: nothing ships it.
                this.automations.local_version = Some(version.unwrap_or_default());
                cx.notify();
            });
        }));
    }

    /// The version this app ships, when it ships one.
    pub(crate) fn shipped_automate_version(&self) -> Option<&str> {
        self.automations
            .local_version
            .as_deref()
            .filter(|version| !version.is_empty())
    }

    /// Whether `host` runs another `herdr-automate` than this app ships.
    pub(crate) fn automation_update(&self, host: &HostState) -> Option<String> {
        let shipped = self.shipped_automate_version()?;
        let status = host.status()?;
        // The daemon that runs is what matters; with none running, the
        // installed program is what `start` would run.
        let running = match &status.daemon.version {
            Some(version) if status.daemon.running => version,
            _ => &status.version,
        };
        (!host.machine.is_local() && running != shipped).then(|| shipped.to_owned())
    }

    /// Shows `opened`, reading the run's page unless it was read.
    pub(crate) fn open_automation(&mut self, opened: Opened, cx: &mut Context<Self>) {
        if let Opened::Run { machine, run, .. } = &opened {
            let read = matches!(
                self.automations.runs.get(&(machine.clone(), *run)),
                Some(Loaded::Read(_) | Loaded::Reading { .. })
            );
            if !read || self.automation_run_live(machine, *run) {
                self.read_automation_run(machine, *run, cx);
            }
        }
        self.automations.opened = Some(opened);
        self.automations.page_scroll.set_offset(Point::default());
        cx.notify();
    }

    pub(crate) fn close_automation(&mut self, cx: &mut Context<Self>) {
        self.automations.opened = None;
        cx.notify();
    }

    pub(crate) fn read_automation_run(
        &mut self,
        machine: &Machine,
        run: i64,
        cx: &mut Context<Self>,
    ) {
        self.automations.trim();
        let Some(host) = self.automations.host(machine) else {
            return;
        };
        let script_host = host.host.clone();
        let cancelled = self.automations.cancelled();
        let work = cx
            .background_executor()
            .spawn(async move { fetch::run_detail(&script_host, run, &cancelled) });
        let key = (machine.clone(), run);
        let installed = key.clone();
        let task = cx.spawn(async move |this, cx| {
            let detail = work.await;
            let _ = this.update(cx, |this, cx| {
                let loaded = match detail {
                    Ok(detail) => Loaded::Read(Arc::new(detail)),
                    Err(error) => Loaded::Failed(Arc::new(error)),
                };
                this.automations.runs.insert(installed, loaded);
                cx.notify();
            });
        });
        // A page already read stays shown while it is read again.
        if !matches!(self.automations.runs.get(&key), Some(Loaded::Read(_))) {
            self.automations
                .runs
                .insert(key, Loaded::Reading { _task: task });
        } else {
            task.detach();
        }
    }

    /// Reads a run's whole log, or one step's, to show in full.
    pub(crate) fn read_automation_log(&mut self, key: LogKey, cx: &mut Context<Self>) {
        self.automations.trim();
        let Some(host) = self.automations.host(&key.machine) else {
            return;
        };
        let script_host = host.host.clone();
        let cancelled = self.automations.cancelled();
        let (run, step) = (key.run, key.step);
        let work = cx
            .background_executor()
            .spawn(async move { fetch::run_log(&script_host, run, step, &cancelled) });
        let installed = key.clone();
        let task = cx.spawn(async move |this, cx| {
            let log = work.await;
            let _ = this.update(cx, |this, cx| {
                let loaded = match log {
                    Ok(log) => Loaded::Read(Arc::new(log)),
                    Err(error) => Loaded::Failed(Arc::new(error)),
                };
                this.automations.logs.insert(installed, loaded);
                cx.notify();
            });
        });
        self.automations
            .logs
            .insert(key, Loaded::Reading { _task: task });
        cx.notify();
    }

    /// Shows or hides a full log, reading it the first time.
    pub(crate) fn toggle_automation_log(&mut self, key: LogKey, cx: &mut Context<Self>) {
        match self.automations.logs.get(&key) {
            Some(Loaded::Read(_) | Loaded::Reading { .. }) => {
                self.automations.logs.remove(&key);
                cx.notify();
            }
            Some(Loaded::Failed(_)) | None => self.read_automation_log(key, cx),
        }
    }
}
