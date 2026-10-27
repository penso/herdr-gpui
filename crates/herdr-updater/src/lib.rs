//! Herdr UI's update service. Workers own all transport, staging, and process
//! waits; the app drives an [`Updater`] from its UI thread and renders its
//! [`State`]. No GPUI: `scripts/test-updater.py` builds and exercises this
//! crate on its own.

// Tests start fixtures directly; production children use herdr_client::new_command.
#![cfg_attr(test, allow(clippy::disallowed_methods))]

mod channel;
mod error;
pub use channel::UpdateChannel;
pub use error::{Error, Result};
pub use error::{quota_exhausted, rate_limit_reset, retry_hint};
#[cfg(unix)]
mod brew;
#[cfg(unix)]
mod install;
#[cfg(target_os = "linux")]
mod linux;
mod release;
#[cfg(not(unix))]
mod unsupported;
#[cfg(not(unix))]
use unsupported::brew;
#[cfg(not(any(unix, windows)))]
use unsupported::install;
#[cfg(windows)]
mod windows;
#[cfg(windows)]
use windows as install;

pub use release::{RELEASES_PAGE, release_page};

use std::{
    ffi::OsString,
    process::ExitCode,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, SyncSender},
    },
    thread,
    time::{Duration, Instant},
};

/// The macOS bundle identifier, also the Linux app ID and Windows
/// AppUserModelID. The Homebrew relaunch reopens the bundle by it.
pub const APP_ID: &str = "so.pen.herdr-gpui";

/// Release builds embed the same calendar version (YYYYMMDD.COUNTER) used for
/// the tag, the bundle, and the downloadable artifacts. Local builds are not
/// releases, so they carry no version the updater or an issue report could
/// act on.
pub const APP_VERSION: &str = match option_env!("HERDR_RELEASE_VERSION") {
    Some(version) => version,
    None => "dev",
};

const CHECK_INTERVAL: Duration = Duration::from_secs(60 * 60);

/// The sample release body the QA preview shows, so the notes section can be
/// reviewed without waiting for a real release. Grouped the way several skipped
/// releases are, so the preview exercises the multi-release form too. Shared
/// with the layout test so the two cannot drift.
pub const PREVIEW_NOTES: &str = "## 20260920.2\n\n### Added\n- Release notes are shown before you update.\n\n## 20260920.1\n\n### Fixed\n- A sample fix, so the preview shows several releases grouped.";

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum State {
    Disabled(String),
    Idle,
    Checking,
    Current,
    Available {
        version: String,
        /// GitHub's release notes for this version, shown before updating.
        /// Untrusted text, not covered by the signed manifest.
        notes: String,
    },
    Downloading {
        received: u64,
        total: u64,
    },
    Ready {
        version: String,
    },
    Installing,
    /// Homebrew owns this installation, so Homebrew performs the upgrade.
    Homebrew {
        version: String,
        notes: String,
    },
    Upgrading {
        detail: String,
    },
    Restart {
        version: String,
    },
    Restarting,
    Cancelling,
    Error(String),
}

impl State {
    fn busy(&self) -> bool {
        matches!(
            self,
            Self::Checking
                | Self::Downloading { .. }
                | Self::Installing
                | Self::Upgrading { .. }
                | Self::Restarting
                | Self::Cancelling
        )
    }
}

#[derive(Clone, Copy)]
enum Operation {
    Check(UpdateChannel),
    Download,
    Install,
    Upgrade,
    Restart,
    Cancel,
}

struct Command {
    generation: u64,
    operation: Operation,
}

struct Mailbox {
    generation: u64,
    state: State,
    restart: Option<RestartGuard>,
    /// The upgraded app is already starting; this one only has to quit.
    relaunched: bool,
}

enum Prepared {
    Standalone(install::Prepared),
    #[cfg(target_os = "linux")]
    Package(linux::Prepared),
}

enum Restart {
    Homebrew,
    #[cfg(target_os = "linux")]
    Package(std::path::PathBuf),
}

enum RestartGuard {
    Standalone(install::RestartGuard),
    #[cfg(target_os = "linux")]
    Package(linux::RestartGuard),
}

impl RestartGuard {
    fn commit(&mut self) -> Result<()> {
        match self {
            Self::Standalone(guard) => guard.commit(),
            #[cfg(target_os = "linux")]
            Self::Package(guard) => guard.commit(),
        }
    }
}

impl Restart {
    fn launch(&self) -> Result<Option<RestartGuard>> {
        match self {
            Self::Homebrew => brew::relaunch(&cask().ok_or(Error::MissingCask)?).map(|()| None),
            #[cfg(target_os = "linux")]
            Self::Package(executable) => {
                linux::relaunch(executable).map(|guard| Some(RestartGuard::Package(guard)))
            }
        }
    }
}

fn installation_format(cancel: &AtomicBool) -> Result<release::Format> {
    #[cfg(target_os = "linux")]
    {
        Ok(linux::detect(cancel)?.map_or(release::Format::Archive, |package| package.format))
    }
    #[cfg(windows)]
    {
        let _ = cancel;
        Ok(release::Format::Installer)
    }
    #[cfg(not(any(target_os = "linux", windows)))]
    {
        let _ = cancel;
        Ok(release::Format::Archive)
    }
}

fn prepare(
    offer: &release::Offer,
    cancel: &AtomicBool,
    progress: impl FnMut(u64, u64),
) -> Result<Prepared> {
    #[cfg(target_os = "linux")]
    if let Some(package) = linux::detect(cancel)? {
        return linux::prepare(package, offer, cancel, progress).map(Prepared::Package);
    }
    install::prepare(offer, cancel, progress).map(Prepared::Standalone)
}

pub struct Updater {
    state: State,
    commands: Option<SyncSender<Command>>,
    cancelled: Arc<AtomicBool>,
    stopped: Arc<AtomicBool>,
    mailbox: Arc<Mutex<Option<Mailbox>>>,
    generation: u64,
    restart: Option<RestartGuard>,
    relaunched: bool,
    committed: bool,
    next_check: Instant,
    channel: UpdateChannel,
    /// The channel of the last queued check, which produced any current offer.
    checked: UpdateChannel,
    latest_version: Option<String>,
}

impl Default for Updater {
    fn default() -> Self {
        Self {
            state: State::Disabled("In-app updates require a release build with an embedded update verification key. Local builds and test fixtures never check automatically.".into()),
            commands: None,
            cancelled: Arc::new(AtomicBool::new(false)),
            stopped: Arc::new(AtomicBool::new(false)),
            mailbox: Arc::new(Mutex::new(None)),
            generation: 0,
            restart: None,
            relaunched: false,
            committed: false,
            next_check: Instant::now() + CHECK_INTERVAL,
            channel: UpdateChannel::default(),
            checked: UpdateChannel::default(),
            latest_version: None,
        }
    }
}

impl Updater {
    /// Additional windows never run a second update worker: two workers could
    /// stage and restart the same installation at once.
    pub fn secondary() -> Self {
        let mut updater = Self::default();
        updater.state = State::Disabled(
            "another window owns app updates in this process. Use the window that was open first."
                .into(),
        );
        updater
    }

    pub fn start(channel: UpdateChannel) -> Self {
        let mut updater = Self::default();
        updater.channel = channel;
        let Some(key) = option_env!("HERDR_UPDATE_PUBLIC_KEY") else {
            return updater;
        };
        if release::parse_version(APP_VERSION).is_none() || key.is_empty() {
            return updater;
        }
        if release::target().is_none() {
            updater.state = State::Disabled("No standalone updater is available for this platform. Use your package manager or download a supported release.".into());
            return updater;
        }
        #[cfg(windows)]
        if let Some(reason) = windows::missing_publisher() {
            updater.state = State::Disabled(reason.to_string());
            return updater;
        }
        // At most one operation and its cancellation acknowledgement are queued.
        let (sender, receiver) = mpsc::sync_channel(2);
        let mailbox = updater.mailbox.clone();
        let cancelled = updater.cancelled.clone();
        let stopped = updater.stopped.clone();
        match thread::Builder::new()
            .name("herdr-updates".into())
            .spawn(move || worker(receiver, mailbox, cancelled, stopped, key))
        {
            Ok(_) => {
                updater.commands = Some(sender);
                updater.state = State::Idle;
                updater.check();
            }
            Err(error) => {
                updater.state = State::Disabled(format!("Could not start updater: {error}"))
            }
        }
        updater
    }

    /// Follows `[updates] channel`. A change rechecks as soon as the worker
    /// is free, so turning betas on offers a waiting one now rather than at
    /// the next hourly check, and turning them off drops a beta offer, even
    /// one already downloaded or still downloading. Neither direction ever
    /// offers a downgrade.
    pub fn set_channel(&mut self, channel: UpdateChannel) {
        if self.channel == channel {
            return;
        }
        self.channel = channel;
        if self.commands.is_some() {
            self.next_check = Instant::now();
        }
    }

    pub fn state(&self) -> &State {
        &self.state
    }

    pub fn latest_version(&self) -> Option<&str> {
        self.latest_version.as_deref()
    }

    /// A newer release is waiting for the user, either still to download or
    /// already staged. Transient checking/downloading states are not a result.
    pub fn update_available(&self) -> bool {
        matches!(
            self.state,
            State::Available { .. }
                | State::Ready { .. }
                | State::Homebrew { .. }
                | State::Restart { .. }
        )
    }

    fn send(&mut self, operation: Operation, state: State) {
        let Some(sender) = &self.commands else { return };
        if self.state.busy() || self.restart.is_some() {
            return;
        }
        let generation = self.generation.wrapping_add(1);
        self.cancelled.store(false, Ordering::Release);
        match sender.try_send(Command {
            generation,
            operation,
        }) {
            Ok(()) => {
                self.generation = generation;
                self.state = state;
                self.next_check = Instant::now() + CHECK_INTERVAL;
                if let Operation::Check(channel) = operation {
                    self.checked = channel;
                }
            }
            Err(mpsc::TrySendError::Full(_)) => (),
            Err(mpsc::TrySendError::Disconnected(_)) => {
                self.state = State::Error("Update worker stopped".into());
            }
        }
    }

    /// A staged download is kept rather than rechecked, unless it came from
    /// a channel the user has since left.
    pub fn check(&mut self) {
        // Package installation has already replaced this executable. Keep its
        // restart offer instead of checking against the old in-memory version.
        if matches!(self.state, State::Restart { .. }) {
            return;
        }
        if !matches!(self.state, State::Ready { .. }) || self.checked != self.channel {
            self.send(Operation::Check(self.channel), State::Checking);
        }
    }

    pub fn download(&mut self) {
        if matches!(self.state, State::Available { .. }) {
            self.send(
                Operation::Download,
                State::Downloading {
                    received: 0,
                    total: 0,
                },
            );
        }
    }

    pub fn install(&mut self) {
        if matches!(self.state, State::Ready { .. }) {
            // Installation cannot be cancelled once a system package transaction
            // starts. The worker publishes its own progress through this state.
            self.send(
                Operation::Install,
                State::Upgrading {
                    detail: "Preparing installation...".into(),
                },
            );
        }
    }

    pub fn upgrade(&mut self) {
        if matches!(self.state, State::Homebrew { .. }) {
            self.send(
                Operation::Upgrade,
                State::Upgrading {
                    detail: "Starting Homebrew...".into(),
                },
            );
        }
    }

    pub fn restart(&mut self) {
        if matches!(self.state, State::Restart { .. }) {
            self.send(Operation::Restart, State::Restarting);
        }
    }

    /// Package managers are never interrupted mid-transaction.
    fn interruptible(&self) -> bool {
        self.state.busy()
            && !matches!(
                self.state,
                State::Cancelling | State::Upgrading { .. } | State::Restarting
            )
    }

    pub fn cancel(&mut self) {
        if self.interruptible() && !self.committed {
            self.cancelled.store(true, Ordering::Release);
            let Some(sender) = &self.commands else { return };
            let generation = self.generation.wrapping_add(1);
            match sender.try_send(Command {
                generation,
                operation: Operation::Cancel,
            }) {
                Ok(()) => {
                    self.generation = generation;
                    self.restart = None;
                    self.state = State::Cancelling;
                }
                Err(_) => {
                    self.state =
                        State::Error("Update worker could not acknowledge cancellation".into())
                }
            }
        }
    }

    pub fn poll(&mut self) -> bool {
        // Scheduling shares the UI-side generation/cancellation path with manual checks.
        let scheduled = if self.commands.is_some() && Instant::now() >= self.next_check {
            let before = self.state.clone();
            self.check();
            before != self.state
        } else {
            false
        };
        let Ok(mut mailbox) = self.mailbox.try_lock() else {
            return scheduled;
        };
        let Some(message) = mailbox.take() else {
            return scheduled;
        };
        if message.generation != self.generation {
            return scheduled;
        }
        let changed =
            self.state != message.state || message.restart.is_some() || message.relaunched;
        self.state = message.state;
        match &self.state {
            State::Available { version, .. }
            | State::Ready { version }
            | State::Homebrew { version, .. }
            | State::Restart { version } => {
                self.latest_version = Some(version.clone());
            }
            State::Current => self.latest_version = Some(APP_VERSION.into()),
            _ => (),
        }
        self.restart = message.restart;
        self.relaunched |= message.relaunched;
        changed || scheduled
    }

    pub fn commit_restart(&mut self) -> Result<bool> {
        if self.committed {
            return Ok(false);
        }
        // Homebrew installations restart by launching the upgraded bundle, so
        // there is no staged helper to hand over to: just quit.
        if self.relaunched {
            self.committed = true;
            return Ok(true);
        }
        let Some(guard) = &mut self.restart else {
            return Ok(false);
        };
        if self.cancelled.load(Ordering::Acquire) {
            self.restart = None;
            self.state = State::Idle;
            return Ok(false);
        }
        if let Err(error) = guard.commit() {
            self.state = State::Error(error.to_string());
            self.restart = None;
            return Err(error);
        }
        self.committed = true;
        Ok(true)
    }
}

impl Drop for Updater {
    fn drop(&mut self) {
        self.stopped.store(true, Ordering::Release);
        self.cancelled.store(true, Ordering::Release);
        // Disconnect, never join. Workers clean up only their own private staging.
        self.commands = None;
    }
}

fn publish(
    mailbox: &Mutex<Option<Mailbox>>,
    generation: u64,
    state: State,
    restart: Option<RestartGuard>,
) {
    publish_relaunch(mailbox, generation, state, restart, false);
}

fn publish_relaunch(
    mailbox: &Mutex<Option<Mailbox>>,
    generation: u64,
    state: State,
    restart: Option<RestartGuard>,
    relaunched: bool,
) {
    if let Ok(mut slot) = mailbox.lock() {
        *slot = Some(Mailbox {
            generation,
            state,
            restart,
            relaunched,
        });
    }
}

/// The Homebrew cask that owns this installation, re-proved on every use: the
/// user may have moved, reinstalled, or removed it since the last check.
fn cask() -> Option<brew::Cask> {
    if !cfg!(target_os = "macos") {
        return None;
    }
    let executable = std::env::current_exe().ok()?;
    let bundle = install::mac_bundle(&executable).ok()?;
    brew::detect(&bundle, install::effective_uid().ok()?)
}

/// The tap only ever carries stable releases, so only the stable channel
/// updates a Homebrew installation through Homebrew. On the beta channel it
/// installs the signed release in place like a standalone one; Homebrew takes
/// the bundle back at its next upgrade.
fn through_homebrew<T>(channel: UpdateChannel, cask: impl FnOnce() -> Option<T>) -> bool {
    channel == UpdateChannel::Stable && cask().is_some()
}

fn version(offer: Option<&release::Offer>) -> Option<&str> {
    offer.map(|offer| offer.manifest.version.as_str())
}

/// A download staged for `previous` is kept only when a recheck offers that
/// same release again, as when a beta is promoted while it waits.
fn restage<T>(staged: Option<T>, previous: Option<&str>, found: Option<&str>) -> Option<T> {
    staged.filter(|_| found.is_some() && found == previous)
}

fn worker(
    commands: Receiver<Command>,
    mailbox: Arc<Mutex<Option<Mailbox>>>,
    cancelled: Arc<AtomicBool>,
    stopped: Arc<AtomicBool>,
    key: &'static str,
) {
    let mut offer: Option<release::Offer> = None;
    let mut prepared = None;
    let mut restart = Restart::Homebrew;
    while let Ok(command) = commands.recv() {
        if stopped.load(Ordering::Acquire) {
            break;
        }
        let generation = command.generation;
        let result: Result<State> = match command.operation {
            Operation::Check(channel) => {
                publish(&mailbox, generation, State::Checking, None);
                // Only a channel change rechecks with a download staged, and
                // that download survives only if the same release is offered.
                let staged = prepared.take();
                // Re-proved on every check, off the UI thread: the user may
                // move or replace the installation while the app runs.
                #[cfg(windows)]
                if let Some(reason) = windows::unsupported() {
                    drop(staged);
                    offer = None;
                    publish(
                        &mailbox,
                        generation,
                        State::Disabled(reason.to_string()),
                        None,
                    );
                    continue;
                }
                let homebrew = through_homebrew(channel, cask);
                installation_format(&cancelled)
                    .and_then(|format| {
                        release::check(APP_VERSION, key, channel, format, &cancelled)
                    })
                    .map(|found| {
                        prepared =
                            restage(staged, version(offer.as_ref()), version(found.as_ref()));
                        offer = found;
                        match (&offer, homebrew, prepared.is_some()) {
                            (Some(offer), true, _) => State::Homebrew {
                                version: offer.manifest.version.clone(),
                                notes: offer.notes.clone(),
                            },
                            (Some(offer), false, true) => State::Ready {
                                version: offer.manifest.version.clone(),
                            },
                            (Some(offer), false, false) => State::Available {
                                version: offer.manifest.version.clone(),
                                notes: offer.notes.clone(),
                            },
                            (None, ..) => State::Current,
                        }
                    })
            }
            Operation::Download => match &offer {
                Some(offer) => prepare(offer, &cancelled, |received, total| {
                    publish(
                        &mailbox,
                        generation,
                        State::Downloading { received, total },
                        None,
                    );
                })
                .map(|candidate| {
                    prepared = Some(candidate);
                    State::Ready {
                        version: offer.manifest.version.clone(),
                    }
                }),
                None => Err(Error::MissingOffer),
            },
            Operation::Install => match prepared.take() {
                Some(Prepared::Standalone(candidate)) => {
                    publish(&mailbox, generation, State::Installing, None);
                    match install::install_and_restart(candidate, &cancelled) {
                        Ok(guard) => {
                            publish(
                                &mailbox,
                                generation,
                                State::Installing,
                                Some(RestartGuard::Standalone(guard)),
                            );
                            // Keep receiving commands: cancellation/commit failure may leave
                            // this app alive even after the helper becomes ready.
                            continue;
                        }
                        Err(error) => Err(error),
                    }
                }
                #[cfg(target_os = "linux")]
                Some(Prepared::Package(candidate)) => {
                    linux::install(candidate, &cancelled, |detail| {
                        publish(&mailbox, generation, State::Upgrading { detail }, None);
                    })
                    .and_then(|executable| {
                        restart = Restart::Package(executable);
                        Ok(State::Restart {
                            version: version(offer.as_ref()).ok_or(Error::MissingOffer)?.into(),
                        })
                    })
                }
                None => Err(Error::MissingPrepared),
            },
            Operation::Upgrade => match (&offer, cask()) {
                (Some(offer), Some(cask)) => brew::upgrade(
                    &cask,
                    APP_VERSION,
                    &offer.manifest.version,
                    &cancelled,
                    |detail| {
                        publish(
                            &mailbox,
                            generation,
                            State::Upgrading {
                                detail: format!("Homebrew: {detail}"),
                            },
                            None,
                        );
                    },
                )
                .map(|version| {
                    restart = Restart::Homebrew;
                    State::Restart { version }
                }),
                (None, _) => Err(Error::MissingOffer),
                (_, None) => Err(Error::MissingCask),
            },
            Operation::Restart => match restart.launch() {
                Ok(Some(guard)) => {
                    publish(&mailbox, generation, State::Restarting, Some(guard));
                    continue;
                }
                Ok(None) => {
                    // The upgraded app is starting, so this one must quit. The
                    // mailbox keeps only the latest message: publishing the
                    // relaunch last is what keeps it from being overwritten.
                    publish_relaunch(&mailbox, generation, State::Restarting, None, true);
                    continue;
                }
                Err(error) => Err(error),
            },
            Operation::Cancel => {
                prepared = None;
                Ok(State::Idle)
            }
        };
        if stopped.load(Ordering::Acquire) {
            break;
        }
        let state = if cancelled.load(Ordering::Acquire) {
            prepared = None;
            State::Idle
        } else {
            // Display text never carries remote diagnostics, so it is safe to log.
            result.unwrap_or_else(|error| {
                tracing::warn!(category = "updater", %error, "update operation failed");
                State::Error(error.to_string())
            })
        };
        publish(&mailbox, generation, state, None);
    }
}

pub fn run_helper(args: &[OsString]) -> Option<ExitCode> {
    #[cfg(target_os = "linux")]
    if let Some(code) = linux::run_helper(args) {
        return Some(code);
    }
    install::run_helper(args)
}

#[cfg(test)]
mod tests;
