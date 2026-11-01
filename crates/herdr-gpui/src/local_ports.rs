//! The activity bar's Ports section: every TCP port this computer's user
//! listens on, whether or not a Herdr pane started it, with the process
//! behind it, and opening, pausing, resuming, or stopping that process.
//!
//! One worker thread lists the ports every few seconds while the section is
//! shown (or a `herdr-gpui ports all` asked recently), and signals processes
//! when the user confirms. The UI thread only starts and drops the worker
//! and takes its answers on later ticks. Only macOS and Linux are listed:
//! Windows has neither `lsof` nor `ss`, nor SIGSTOP, and says so.
// i18n: migrated

mod error;
#[cfg(unix)]
mod process;
#[cfg(unix)]
mod scan;
mod section;
#[cfg(unix)]
mod worker;

#[cfg(test)]
mod tests;

pub(crate) use error::Error;

use crate::{
    Result,
    browser::WebUrl,
    listening_ports::{Bind, Link, Origin, Port},
    processes::Identity,
};
use herdr_control_protocol::protocol::hosts::PortAction;
use std::{
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::{Duration, Instant, SystemTime},
};

/// Whether this platform can list and signal its ports at all.
pub(crate) const SUPPORTED: bool = cfg!(unix);
/// Ports open and close at human pace, as the workspace chips are scanned.
const INTERVAL: Duration = Duration::from_secs(5);
/// How long a command line request keeps the ports scanned with the section
/// closed, so a script asking again finds a fresh listing.
pub(crate) const DEMAND: Duration = Duration::from_secs(60);
/// Rows kept, lowest port first.
const ROW_LIMIT: usize = 512;

/// One port a process of this user listens on.
// Only the Unix worker makes these; Windows lists none.
#[cfg_attr(not(unix), allow(dead_code))]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Listener {
    pub identity: Identity,
    pub number: u16,
    pub bind: Bind,
    /// The process's name, bounded display text.
    pub process: String,
    /// Its working directory, when the OS says.
    pub directory: Option<PathBuf>,
    /// Stopped by SIGSTOP or a debugger.
    pub paused: bool,
}

impl Listener {
    fn port(&self) -> Port {
        Port {
            number: self.number,
            bind: self.bind,
            process: self.process.clone(),
        }
    }

    /// `*:3000`, `localhost:3000`, or the one address it listens on.
    pub(crate) fn address(&self) -> String {
        self.port().address()
    }

    /// Where a browser on this computer reaches it.
    pub(crate) fn url(&self) -> Option<WebUrl> {
        match self.port().link(&Origin::Local)? {
            Link::Page(url) => Some(url),
            Link::Tunnel(_) => None,
        }
    }

    /// How long its process has run, by the OS's start time.
    pub(crate) fn uptime(&self, now: SystemTime) -> Option<Duration> {
        let started = SystemTime::UNIX_EPOCH + Duration::from_secs(self.identity.started);
        now.duration_since(started).ok()
    }
}

/// `path` from the home folder, as a shell prompt shows it: `~/app`.
pub(crate) fn home_relative(path: &Path, home: Option<&Path>) -> String {
    match home.and_then(|home| path.strip_prefix(home).ok()) {
        Some(rest) if rest.as_os_str().is_empty() => "~".to_owned(),
        Some(rest) => format!("~/{}", rest.display()),
        None => path.display().to_string(),
    }
}

/// What became of one process the user asked to signal.
#[cfg_attr(not(unix), allow(dead_code))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Outcome {
    /// Signalled; a stopped one exited within its grace period.
    Signalled,
    /// Stopped only by SIGKILL, after ignoring SIGTERM.
    Killed,
    /// The OS refused the signal.
    Failed,
    /// Its pid is gone, or now names another process; nothing was sent.
    Exited,
    /// It belongs to another user; nothing was sent.
    Foreign,
}

/// A signal the user is asked to confirm, and the rows it goes to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Pending {
    pub action: PortAction,
    pub listeners: Vec<Listener>,
}

impl Pending {
    /// The processes, once each, in the order listed.
    pub(crate) fn victims(&self) -> Vec<Identity> {
        let mut victims: Vec<Identity> = Vec::new();
        for listener in &self.listeners {
            if !victims.contains(&listener.identity) {
                victims.push(listener.identity);
            }
        }
        victims
    }
}

/// How the last signal went, for the section to say.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Report {
    pub action: Option<PortAction>,
    /// Signalled, or stopped.
    pub done: usize,
    /// Of which SIGKILL ended.
    pub killed: usize,
    pub gone: usize,
    pub failed: usize,
}

impl Report {
    fn of(action: PortAction, outcomes: &[(Identity, Outcome)]) -> Self {
        let mut report = Self {
            action: Some(action),
            ..Self::default()
        };
        for (_, outcome) in outcomes {
            match outcome {
                Outcome::Signalled => report.done += 1,
                Outcome::Killed => {
                    report.done += 1;
                    report.killed += 1;
                }
                Outcome::Exited => report.gone += 1,
                Outcome::Failed | Outcome::Foreign => report.failed += 1,
            }
        }
        report
    }
}

#[derive(Debug, PartialEq)]
pub(crate) enum Command {
    Refresh,
    Signal(PortAction, Vec<Identity>),
}

#[cfg_attr(not(unix), allow(dead_code))]
pub(crate) enum Update {
    Listing(Result<Vec<Listener>>),
    Signalled(PortAction, Vec<(Identity, Outcome)>),
}

/// The running worker's channels. Dropping it stops the worker without
/// waiting for it.
pub(crate) struct Worker {
    commands: mpsc::SyncSender<Command>,
    updates: mpsc::Receiver<Update>,
    cancelled: Arc<AtomicBool>,
}

impl Drop for Worker {
    /// Abandons a scan in flight; the closed channel ends the loop.
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::Release);
    }
}

#[cfg(unix)]
fn start() -> Result<Worker> {
    worker::start()
}

#[cfg(not(unix))]
fn start() -> Result<Worker> {
    Err(Error::Unsupported.into())
}

/// The window's view of this computer's ports.
#[derive(Default)]
pub(crate) struct LocalPorts {
    /// None while nothing asks for ports, or when it could not start.
    worker: Option<Worker>,
    /// Whether starting failed while still wanted; tried again once the
    /// section is reopened.
    start_failed: bool,
    /// The latest listing, kept while closed so reopening shows it at once.
    listing: Option<Vec<Listener>>,
    /// Why the latest scan failed, as display text; the last good listing
    /// stays shown.
    error: Option<String>,
    /// A signal is on its way.
    acting: bool,
    report: Option<Report>,
    /// Until when a command line request keeps the scan running.
    demand: Option<Instant>,
}

/// `error` and its causes, for display.
fn describe(error: &crate::Error) -> String {
    let mut text = error.to_string();
    let mut source = std::error::Error::source(error);
    while let Some(cause) = source {
        text.push(' ');
        text.push_str(&cause.to_string());
        source = cause.source();
    }
    text
}

impl LocalPorts {
    /// Starts or drops the worker as `shown` (or a recent request) wants,
    /// and takes its answers. Whether anything shown changed.
    pub(crate) fn poll(&mut self, shown: bool, now: Instant) -> bool {
        let wanted = SUPPORTED
            && (shown || self.acting || self.demand.is_some_and(|until| now < until));
        if !wanted {
            self.demand = None;
            self.start_failed = false;
            // Dropping it stops the thread, after a signal under way; the
            // listing stays for next time.
            if self.worker.take().is_some() {
                self.acting = false;
            }
            return false;
        }
        if self.worker.is_none() && !self.start_failed {
            match start() {
                Ok(worker) => self.worker = Some(worker),
                Err(error) => {
                    tracing::warn!(category = "local-ports", error = %describe(&error), "could not start the port list");
                    self.error = Some(describe(&error));
                    self.start_failed = true;
                    return true;
                }
            }
        }
        let mut changed = false;
        while let Some(worker) = &self.worker {
            match worker.updates.try_recv() {
                Ok(update) => {
                    self.apply(update);
                    changed = true;
                }
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.worker = None;
                    self.acting = false;
                    self.error = Some(describe(&Error::Stopped.into()));
                    self.start_failed = true;
                    changed = true;
                }
            }
        }
        changed
    }

    fn apply(&mut self, update: Update) {
        match update {
            Update::Listing(Ok(mut listing)) => {
                listing.truncate(ROW_LIMIT);
                self.listing = Some(listing);
                self.error = None;
            }
            Update::Listing(Err(error)) => {
                let text = describe(&error);
                if self.error.as_deref() != Some(text.as_str()) {
                    tracing::warn!(category = "local-ports", error = %text, "could not list this computer's ports");
                }
                self.error = Some(text);
            }
            Update::Signalled(action, outcomes) => {
                self.acting = false;
                self.report = Some(Report::of(action, &outcomes));
            }
        }
    }

    /// The latest listing; `None` before the first scan.
    pub(crate) fn listing(&self) -> Option<&[Listener]> {
        self.listing.as_deref()
    }

    /// Why the latest scan failed.
    pub(crate) fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    pub(crate) fn acting(&self) -> bool {
        self.acting
    }

    pub(crate) fn report(&self) -> Option<Report> {
        self.report
    }

    /// Whether a scan is running, so a refresh can be asked of it.
    pub(crate) fn running(&self) -> bool {
        self.worker.is_some()
    }

    /// Keeps the ports scanned until `until`, for a command line request.
    pub(crate) fn demand(&mut self, until: Instant) {
        self.demand = Some(self.demand.map_or(until, |known| known.max(until)));
    }

    /// Lists the ports again now rather than at the next interval. A scan
    /// already queued makes this one unnecessary.
    pub(crate) fn refresh(&mut self) {
        if let Some(worker) = &self.worker {
            let _ = worker.commands.try_send(Command::Refresh);
        }
    }

    /// Asks the worker to signal exactly `victims`, after checking each is
    /// still the process listed.
    pub(crate) fn signal(&mut self, action: PortAction, victims: Vec<Identity>) -> Result<()> {
        if self.acting {
            return Err(Error::Busy.into());
        }
        // Confirmed after the scan stopped, as a command's question can be.
        if self.worker.is_none() {
            self.worker = Some(start()?);
        }
        let worker = self.worker.as_ref().ok_or(Error::Stopped)?;
        worker
            .commands
            .try_send(Command::Signal(action, victims))
            .map_err(|error| match error {
                mpsc::TrySendError::Full(_) => Error::Busy,
                mpsc::TrySendError::Disconnected(_) => Error::Stopped,
            })?;
        self.acting = true;
        self.report = None;
        Ok(())
    }

    /// Shows `listing` as if a scan had found it, with no worker.
    #[cfg(test)]
    pub(crate) fn seed(&mut self, listing: Vec<Listener>) {
        self.listing = Some(listing);
    }

    /// Ports with no worker thread: the test plays the worker's part.
    #[cfg(test)]
    pub(crate) fn fake() -> (Self, mpsc::SyncSender<Update>, mpsc::Receiver<Command>) {
        let (commands, received) = mpsc::sync_channel(2);
        let (sender, updates) = mpsc::sync_channel(4);
        let worker = Worker {
            commands,
            updates,
            cancelled: Arc::new(AtomicBool::new(false)),
        };
        (
            Self {
                worker: Some(worker),
                ..Self::default()
            },
            sender,
            received,
        )
    }
}
