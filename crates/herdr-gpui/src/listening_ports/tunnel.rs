//! SSH tunnels that make a remote server reachable from a browser tab here
//! at `127.0.0.1`. One `ssh -N -L` child per remote port, started when its
//! port is clicked and kept while the port is still listed, so reopening the
//! page reuses it.

use crate::Result;
use std::{
    collections::HashMap,
    io::{BufRead, BufReader, Read},
    net::{IpAddr, Ipv4Addr, TcpListener},
    num::NonZeroU16,
    process::{Child, ExitStatus, Stdio},
    sync::{Arc, Mutex, MutexGuard, PoisonError, mpsc},
    thread,
    time::{Duration, Instant},
};

/// Why an SSH tunnel did not open.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("SSH ended before the tunnel opened ({0}).")]
    Exited(ExitStatus),
    #[error("No free local port for an SSH tunnel.")]
    Port(#[source] std::io::Error),
    #[error("Could not start ssh for a tunnel.")]
    Start(#[source] std::io::Error),
    #[error("The SSH tunnel did not open in time.")]
    Timeout,
}

/// SSH connects, authenticates, and binds within its own `ConnectTimeout`.
const OPEN_TIMEOUT: Duration = Duration::from_secs(15);
const POLL_INTERVAL: Duration = Duration::from_millis(100);
/// Under `-N`, SSH's stdout carries only the readiness line, perhaps after
/// whatever the user's shell prints when it runs `LocalCommand`.
const READY_OUTPUT: u64 = 64 * 1024;
/// Tunnels are opened by hand, one click each; this only bounds a runaway.
const TUNNEL_LIMIT: usize = 32;

/// A remote port, by the SSH target its host is dialled with.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct Key {
    pub target: String,
    /// The one address the server listens on, or None for one listening on
    /// every address or on loopback, which the remote's `localhost` reaches.
    pub address: Option<IpAddr>,
    pub port: u16,
}

/// The SSH child of a tunnel, shared so quitting or closing the port can kill
/// it at once while the background task alone waits for it, and while its
/// finished [`Tunnel`] is still on its way to the UI thread. Neither side
/// holds the lock across a blocking call.
#[derive(Clone, Default)]
pub(crate) struct Attempt(Arc<Mutex<Slot>>);

#[derive(Default)]
struct Slot {
    cancelled: bool,
    child: Option<Child>,
}

impl Attempt {
    fn slot(&self) -> MutexGuard<'_, Slot> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Takes the freshly spawned child. One that was cancelled meanwhile is
    /// killed at once, which the waiting task then sees as an exit.
    pub(super) fn adopt(&self, child: Child) {
        let mut slot = self.slot();
        let cancelled = slot.cancelled;
        slot.child = Some(child);
        if cancelled {
            kill(&mut slot);
        }
    }

    /// The child's exit, if it has one yet; never blocks.
    pub(super) fn try_wait(&self) -> std::io::Result<Option<ExitStatus>> {
        match self.slot().child.as_mut() {
            Some(child) => child.try_wait(),
            None => Ok(None),
        }
    }

    /// Kills the child without waiting for it; the task that owns the
    /// attempt reaps it. Safe on the UI thread and while quitting.
    pub(crate) fn cancel(&self) {
        let mut slot = self.slot();
        slot.cancelled = true;
        kill(&mut slot);
    }
}

fn kill(slot: &mut Slot) {
    if let Some(child) = &mut slot.child {
        // A child already reaped is not signalled again.
        let _ = child.kill();
    }
}

/// A running `ssh -N -L` child and the local port it forwards. The child
/// stays in its [`Attempt`], so cancelling that attempt still kills it.
pub(crate) struct Tunnel {
    attempt: Attempt,
    local: u16,
}

impl Tunnel {
    pub(super) fn new(attempt: &Attempt, local: u16) -> Self {
        Self {
            attempt: attempt.clone(),
            local,
        }
    }

    /// Whether the child is still forwarding. Never blocks.
    fn alive(&self) -> bool {
        self.attempt
            .slot()
            .child
            .as_mut()
            .is_some_and(|child| matches!(child.try_wait(), Ok(None)))
    }
}

impl Drop for Tunnel {
    /// Kills the child and reaps it on a thread of its own, so closing a
    /// tunnel never waits on the UI thread.
    fn drop(&mut self) {
        let Some(mut child) = self.attempt.slot().child.take() else {
            return;
        };
        let _ = child.kill();
        let reaped = thread::Builder::new()
            .name("herdr-tunnel-reaper".into())
            .spawn(move || {
                let _ = child.wait();
            });
        if let Err(error) = reaped {
            tracing::warn!(category = "listening-ports", %error, "could not reap an SSH tunnel");
        }
    }
}

/// Starts a tunnel to `key` and waits until SSH says its forward is bound,
/// which blocks for up to [`OPEN_TIMEOUT`]: call it off the UI thread.
/// `preferred` is the local port [`Tunnels::begin`] chose, kept when it is
/// still free, so the server keeps its own number or a page reopened after
/// a dropped connection keeps its address.
/// `attempt` lets the caller kill the SSH child while this still waits.
pub(crate) fn open(key: &Key, preferred: Option<u16>, attempt: &Attempt) -> Result<Tunnel> {
    match start(key, preferred, attempt) {
        // Someone took the preferred port meanwhile; any free one will do,
        // unless the attempt was cancelled, which also looks like an exit.
        Err(crate::Error::Tunnel(Error::Exited(_)))
            if preferred.is_some() && !attempt.slot().cancelled =>
        {
            start(key, None, attempt)
        }
        result => result,
    }
}

fn start(key: &Key, preferred: Option<u16>, attempt: &Attempt) -> Result<Tunnel> {
    let local = free_port(preferred).map_err(Error::Port)?;
    let mut command = herdr_client::forward_command(&key.target, local, key.address, key.port)?;
    // Only the readiness line is read; diagnostics may carry secrets.
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    let mut child = command.spawn().map_err(Error::Start)?;
    let stdout = child.stdout.take();
    attempt.adopt(child);
    let result = match stdout {
        Some(stdout) => ready(|| attempt.try_wait(), stdout, Instant::now() + OPEN_TIMEOUT),
        None => Err(Error::Start(std::io::Error::other("no stdout pipe")).into()),
    };
    // From here on, dropping the tunnel kills the child, on failure too.
    let tunnel = Tunnel::new(attempt, local);
    result.map(|()| tunnel)
}

/// Waits for the child, which `exited` polls, to print
/// [`herdr_client::FORWARD_READY`]. The port
/// itself is never probed: a process that bound it before SSH could would
/// answer a probe, and a page opened then would show that process instead.
/// SSH prints the line only once it holds the port.
pub(super) fn ready(
    mut exited: impl FnMut() -> std::io::Result<Option<ExitStatus>>,
    stdout: impl Read + Send + 'static,
    deadline: Instant,
) -> Result<()> {
    let (sender, found) = mpsc::sync_channel(1);
    thread::Builder::new()
        .name("herdr-tunnel-ready".into())
        .spawn(move || {
            let ready = BufReader::new(stdout.take(READY_OUTPUT))
                .lines()
                .map_while(std::io::Result::ok)
                .any(|line| line.trim_end() == herdr_client::FORWARD_READY);
            let _ = sender.send(ready);
        })
        .map_err(Error::Start)?;
    let mut closed = false;
    loop {
        if closed {
            thread::sleep(POLL_INTERVAL);
        } else {
            match found.recv_timeout(POLL_INTERVAL) {
                Ok(true) => return Ok(()),
                // Output ended without the line: the child is exiting.
                Ok(false) | Err(mpsc::RecvTimeoutError::Disconnected) => closed = true,
                Err(mpsc::RecvTimeoutError::Timeout) => {}
            }
        }
        if let Some(status) = exited().map_err(Error::Start)? {
            return Err(Error::Exited(status).into());
        }
        if Instant::now() >= deadline {
            return Err(Error::Timeout.into());
        }
    }
}

/// A loopback port nobody listens on, `preferred` when it is free. The
/// listener is closed before `ssh` binds the port; another process taking it
/// in between makes `ssh` exit before it reports ready, so [`open`] fails
/// rather than opening the other process's page.
pub(super) fn free_port(preferred: Option<u16>) -> std::io::Result<u16> {
    pick(preferred, |port| {
        TcpListener::bind((Ipv4Addr::LOCALHOST, port))?
            .local_addr()
            .map(|address| address.port())
    })
}

/// `preferred` when `bind` takes it, else whatever port `bind(0)` gets.
pub(super) fn pick(
    preferred: Option<u16>,
    mut bind: impl FnMut(u16) -> std::io::Result<u16>,
) -> std::io::Result<u16> {
    match preferred.map(&mut bind) {
        Some(Ok(port)) => Ok(port),
        _ => bind(0),
    }
}

/// This window's tunnels, and the ones still opening.
#[derive(Default)]
pub(crate) struct Tunnels {
    open: HashMap<Key, Tunnel>,
    opening: HashMap<Key, Attempt>,
    /// The local port each remote port was last forwarded to.
    used: HashMap<Key, u16>,
}

impl Tunnels {
    /// The local port of a live tunnel to `key`; a dead one is dropped.
    pub(crate) fn local(&mut self, key: &Key) -> Option<u16> {
        let tunnel = self.open.get(key)?;
        if tunnel.alive() {
            return Some(tunnel.local);
        }
        self.open.remove(key);
        None
    }

    /// The local port of a live tunnel to `key`, leaving a dead one for
    /// [`Tunnels::local`] or [`Tunnels::retain`] to drop. Only the control
    /// server, which is Unix-only, reads it.
    pub(crate) fn live(&self, key: &Key) -> Option<u16> {
        self.open
            .get(key)
            .filter(|tunnel| tunnel.alive())
            .map(|tunnel| tunnel.local)
    }

    /// Marks `key` as opening and returns the local port to prefer, or None
    /// when it is opening already or too many tunnels are. That is the local
    /// port `key` was last forwarded to, else the server's own number as the
    /// host menu's Forward port picks it, unless another remote port last
    /// used that one, whose page may still be open on it.
    pub(crate) fn begin(&mut self, key: &Key) -> Option<Option<u16>> {
        if self.open.len() + self.opening.len() >= TUNNEL_LIMIT || self.opening.contains_key(key) {
            return None;
        }
        self.opening.insert(key.clone(), Attempt::default());
        let preferred = self.used.get(key).copied().or_else(|| {
            NonZeroU16::new(key.port)
                .map(herdr_client::preferred_local_port)
                .filter(|port| !self.used.values().any(|used| used == port))
        });
        Some(preferred)
    }

    /// The handle on the attempt [`Tunnels::begin`] started for `key`.
    pub(crate) fn attempt(&self, key: &Key) -> Option<Attempt> {
        self.opening.get(key).cloned()
    }

    /// Records a finished attempt. A tunnel nobody wants any more, because
    /// its port closed while it was opening, is dropped and None returned.
    pub(crate) fn finish(&mut self, key: Key, tunnel: Tunnel) -> Option<u16> {
        self.opening.remove(&key)?;
        let local = tunnel.local;
        // A local port belongs to one remote port at a time, so a page left
        // open on another remote port's old address is never handed this one.
        self.used.retain(|_, used| *used != local);
        self.used.insert(key.clone(), local);
        self.open.insert(key, tunnel);
        Some(local)
    }

    /// Whether a live tunnel to `key` is open. Dead ones are dropped on the
    /// next [`Tunnels::retain`], so this may lag a just-exited child briefly.
    pub(crate) fn is_open(&self, key: &Key) -> bool {
        self.open.contains_key(key)
    }

    /// Closes the tunnel to `key` by the user's choice: its child is killed
    /// and reaped, and one still opening is forgotten so its result is
    /// dropped. The remembered local port is kept so reopening prefers it.
    /// Returns whether there was anything to close.
    pub(crate) fn close(&mut self, key: &Key) -> bool {
        let open = self.open.remove(key).is_some();
        let opening = self.opening.remove(key).inspect(Attempt::cancel).is_some();
        open || opening
    }

    /// Records a failed attempt. Returns whether anyone still wanted it: not
    /// when the user closed the port meanwhile, which needs no complaint.
    pub(crate) fn fail(&mut self, key: &Key) -> bool {
        self.opening.remove(key).is_some()
    }

    /// Closes every tunnel whose remote port `listening` no longer lists or
    /// whose child has exited, and cancels those still opening, so their
    /// result is dropped too. Returns whether an open tunnel went away.
    pub(crate) fn retain(&mut self, mut listening: impl FnMut(&Key) -> bool) -> bool {
        let before = self.open.len();
        self.open
            .retain(|key, tunnel| tunnel.alive() && listening(key));
        self.opening.retain(|key, attempt| {
            let keep = listening(key);
            if !keep {
                attempt.cancel();
            }
            keep
        });
        self.used.retain(|key, _| listening(key));
        self.open.len() != before
    }

    /// Kills every tunnel's SSH child at once, those still connecting too,
    /// and forgets them so a result arriving later is dropped. Reaping
    /// happens on threads of their own, so this never waits.
    pub(crate) fn close_all(&mut self) {
        self.opening.values().for_each(Attempt::cancel);
        self.open.clear();
        self.opening.clear();
        self.used.clear();
    }

    #[cfg(all(test, unix))]
    pub(crate) fn insert(&mut self, key: Key, tunnel: Tunnel) {
        self.open.insert(key, tunnel);
    }

    #[cfg(all(test, unix))]
    pub(crate) fn len(&self) -> usize {
        self.open.len()
    }
}

impl Drop for Tunnels {
    /// A window that goes away takes its connecting tunnels with it.
    fn drop(&mut self) {
        self.close_all();
    }
}

#[cfg(all(test, unix))]
impl Tunnel {
    /// A tunnel around any child, standing in for `ssh` in tests.
    pub(crate) fn around(child: Child, local: u16) -> Self {
        let attempt = Attempt::default();
        attempt.adopt(child);
        Self::new(&attempt, local)
    }
}
