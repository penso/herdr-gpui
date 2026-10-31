//! The window's link to herdr-core: found running, or started from the copy
//! bundled beside the app when the window is allowed to, on a thread of its
//! own. The window reads the outcome on its tick.

use super::Error;
use herdr_core_client::{Core, CoreBackend, Endpoint, PROGRAM};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};

/// Scrollback lines each herdr-core terminal keeps in the app.
const SCROLLBACK: usize = 10_000;
/// How long a started core has to listen.
const START_TIMEOUT: Duration = Duration::from_secs(20);
/// How long a failed join waits before restored tabs try again, so a core
/// that is not running costs one attempt now and then, not one per tick.
const JOIN_RETRY: Duration = Duration::from_secs(2);

/// Where the window's herdr-core connection stands.
#[derive(Default)]
pub(crate) enum State {
    #[default]
    Idle,
    Connecting(std::sync::mpsc::Receiver<Result<CoreBackend, Error>>),
    Ready(CoreBackend),
    /// Shared so the window can show it without moving it.
    Unavailable(Arc<Error>),
}

/// Whether the window may start a core when none is running.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Start {
    /// Only join a core that already runs.
    Join,
    /// Start the user's `herdr-core`, else the bundled one. The window asks
    /// for this only when no Herdr 1 is installed, or the user did.
    IfMissing,
}

/// How an attempt joins: the real one dials herdr-core; tests stand in.
type Join = fn(Start) -> Result<CoreBackend, Error>;

pub(crate) struct Hub {
    /// What the attempt in flight may do.
    attempt: Option<Start>,
    /// When the last attempt failed.
    failed: Option<Instant>,
    join: Join,
    state: State,
    /// A start asked for while a join-only attempt was in flight; it runs if
    /// that attempt fails, rather than being dropped.
    wants_start: bool,
}

impl Default for Hub {
    fn default() -> Self {
        Self::with_join(join)
    }
}

impl Hub {
    fn with_join(join: Join) -> Self {
        Self {
            attempt: None,
            failed: None,
            join,
            state: State::Idle,
            wants_start: false,
        }
    }

    pub(crate) fn state(&self) -> &State {
        &self.state
    }

    pub(crate) fn backend(&self) -> Option<&CoreBackend> {
        match &self.state {
            State::Ready(backend) => Some(backend),
            _ => None,
        }
    }

    /// Connects on a background thread unless already connected or trying.
    /// A join-only ask waits a moment after a failure; asking to start
    /// always tries, and upgrades a join-only attempt in flight.
    pub(crate) fn connect(&mut self, start: Start) {
        match &self.state {
            State::Ready(_) => return,
            State::Connecting(_) => {
                self.wants_start |= start == Start::IfMissing && self.attempt == Some(Start::Join);
                return;
            }
            State::Unavailable(_)
                if start == Start::Join
                    && self
                        .failed
                        .is_some_and(|failed| failed.elapsed() < JOIN_RETRY) =>
            {
                return;
            }
            State::Idle | State::Unavailable(_) => {}
        }
        self.attempt = Some(start);
        let join = self.join;
        let (send, receive) = std::sync::mpsc::sync_channel(1);
        let spawned = std::thread::Builder::new()
            .name("herdr-core-connect".into())
            .spawn(move || {
                let _ = send.send(join(start));
            });
        self.state = match spawned {
            Ok(_) => State::Connecting(receive),
            Err(error) => State::Unavailable(Arc::new(Error::Worker(error))),
        };
    }

    /// Takes the connection's outcome; true when the state changed. A lost
    /// connection reads as idle again, so the next ask reconnects.
    pub(crate) fn poll(&mut self) -> bool {
        match &self.state {
            State::Connecting(outcome) => match outcome.try_recv() {
                Ok(Ok(backend)) => self.state = State::Ready(backend),
                Ok(Err(_)) if std::mem::take(&mut self.wants_start) => {
                    self.state = State::Idle;
                    self.connect(Start::IfMissing);
                    return false;
                }
                Ok(Err(error)) => {
                    self.failed = Some(Instant::now());
                    self.state = State::Unavailable(Arc::new(error));
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => return false,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => self.state = State::Idle,
            },
            State::Ready(backend) if backend.core().is_lost() => self.state = State::Idle,
            _ => return false,
        }
        true
    }
}

/// Blocking: joins the session's core, starting one first when allowed.
fn join(start: Start) -> Result<CoreBackend, Error> {
    let endpoint = Endpoint::resolve(herdr_client::login_env::var)?;
    let first = match Core::connect(&endpoint) {
        Ok(core) => return Ok(CoreBackend::new(core, SCROLLBACK)),
        Err(error) => error,
    };
    if start == Start::Join || !matches!(first, herdr_core_client::Error::Connect { .. }) {
        return Err(first.into());
    }
    let binary = crate::bundled::locate(PROGRAM).ok_or(Error::NotBundled)?;
    herdr_core_client::start(&binary, herdr_client::login_env::apply, START_TIMEOUT)?;
    // `server start` returns once the core listens; a slow file system can
    // still refuse the first dial.
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        match Core::connect(&endpoint) {
            Ok(core) => return Ok(CoreBackend::new(core, SCROLLBACK)),
            Err(herdr_core_client::Error::Connect { .. }) if Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(error) => return Err(error.into()),
        }
    }
}

#[cfg(test)]
mod tests;
