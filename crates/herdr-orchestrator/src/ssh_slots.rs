//! How many SSH connections the orchestrator's workers open to one host at
//! once. Each project a view lists has its own worker, and they all start,
//! sync, and refresh together, so a host with many projects would otherwise
//! see a burst of new connections. sshd's default `MaxStartups 10:30:100`
//! starts dropping connections that are still authenticating past ten; a
//! dropped one would fail its project. The workers of every window share
//! these slots, which leave room under that limit for the endpoint's own
//! bridge and the user's terminals. Local scripts take no slot.
//!
//! A connection sshd dropped anyway, before key exchange, ran nothing on the
//! host, so it is tried once more after a short pause.

use super::{Error, Result};
use herdr_client::ConnectTarget;
use std::{
    collections::HashMap,
    sync::{
        Condvar, Mutex, MutexGuard, PoisonError,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

/// Connections the workers open to one SSH host at once.
const PER_HOST: usize = 4;
/// How long a dropped connection waits before its one retry, holding no slot.
const RETRY_AFTER: Duration = Duration::from_secs(1);
/// How often a wait looks at its worker's cancellation.
const POLL: Duration = Duration::from_millis(100);

/// The slots every orchestrator worker takes.
static SHARED: Slots = Slots::new(PER_HOST);

/// Runs `run`, which opens at most one SSH connection to `target` at a time,
/// in one of that host's shared slots. See [`Slots::run`].
pub(super) fn run<T>(
    target: &ConnectTarget,
    cancelled: &AtomicBool,
    run: impl FnMut() -> Result<T>,
) -> Result<T> {
    let host = match target {
        ConnectTarget::Ssh { target, .. } => Some(target.as_str()),
        _ => None,
    };
    SHARED.run(host, RETRY_AFTER, cancelled, run)
}

/// Per-host counting semaphores, keyed by SSH destination. A host is listed
/// only while one of its slots is taken, so the map stays as small as the
/// hosts in use.
pub(super) struct Slots {
    limit: usize,
    taken: Mutex<Option<HashMap<String, usize>>>,
    freed: Condvar,
}

impl Slots {
    pub(super) const fn new(limit: usize) -> Self {
        Self {
            limit,
            taken: Mutex::new(None),
            freed: Condvar::new(),
        }
    }

    fn lock(&self) -> MutexGuard<'_, Option<HashMap<String, usize>>> {
        // Counts are changed only after every fallible step, so a panic
        // elsewhere leaves them consistent.
        self.taken.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Takes one of `host`'s slots, waiting while all are taken, until
    /// `cancelled` is set.
    pub(super) fn take<'a>(&'a self, host: &str, cancelled: &AtomicBool) -> Result<Slot<'a>> {
        let mut taken = self.lock();
        loop {
            if cancelled.load(Ordering::Relaxed) {
                return Err(Error::Cancelled);
            }
            let hosts = taken.get_or_insert_with(HashMap::new);
            let count = hosts.entry(host.to_owned()).or_insert(0);
            if *count < self.limit {
                *count += 1;
                return Ok(Slot {
                    slots: self,
                    host: host.to_owned(),
                });
            }
            taken = self
                .freed
                .wait_timeout(taken, POLL)
                .unwrap_or_else(PoisonError::into_inner)
                .0;
        }
    }

    /// Runs `run` in one of `host`'s slots, or straight away for a local
    /// script (`None`). When sshd dropped its connection before key exchange,
    /// it runs once more after `retry_after`, waited out without a slot so
    /// the host's other work goes on.
    pub(super) fn run<T>(
        &self,
        host: Option<&str>,
        retry_after: Duration,
        cancelled: &AtomicBool,
        mut run: impl FnMut() -> Result<T>,
    ) -> Result<T> {
        let Some(host) = host else {
            return run();
        };
        let first = {
            let _slot = self.take(host, cancelled)?;
            run()
        };
        match first {
            Err(Error::Script { source, .. }) if source.dropped_before_key_exchange() => {
                tracing::debug!(
                    category = "orchestrator",
                    "SSH host dropped a connection before key exchange; trying again"
                );
                pause(retry_after, cancelled)?;
                let _slot = self.take(host, cancelled)?;
                run()
            }
            result => result,
        }
    }
}

/// Waits `duration`, or until `cancelled` is set.
fn pause(duration: Duration, cancelled: &AtomicBool) -> Result<()> {
    let until = Instant::now() + duration;
    loop {
        if cancelled.load(Ordering::Relaxed) {
            return Err(Error::Cancelled);
        }
        let left = until.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return Ok(());
        }
        std::thread::sleep(left.min(POLL));
    }
}

/// One taken slot, given back when dropped.
pub(super) struct Slot<'a> {
    slots: &'a Slots,
    host: String,
}

impl Drop for Slot<'_> {
    fn drop(&mut self) {
        let mut taken = self.slots.lock();
        if let Some(hosts) = taken.as_mut()
            && let Some(count) = hosts.get_mut(&self.host)
        {
            *count = count.saturating_sub(1);
            if *count == 0 {
                hosts.remove(&self.host);
            }
        }
        drop(taken);
        self.slots.freed.notify_all();
    }
}
