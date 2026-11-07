//! Latency and loss from this machine to the internet, for the status bar.
//! A worker thread opens one TCP connection every fifteen seconds and times
//! the handshake. That is a few hundred bytes a probe and about a megabyte a
//! day, so a metered link such as a phone hotspot can afford it, unlike a
//! throughput test. The UI thread only starts and stops the worker and takes
//! its probes on later ticks.
// i18n: migrated

mod probe;
mod render;

#[cfg(test)]
mod tests;

use crate::system_load::Stop;
use gpui::{App, Global};
use probe::Outcome;
use std::{
    collections::VecDeque,
    net::{Ipv4Addr, SocketAddr, SocketAddrV4},
    sync::{Arc, mpsc},
    thread,
    time::{Duration, Instant, SystemTime},
};

const INTERVAL: Duration = Duration::from_secs(15);
/// Cloudflare's resolver over HTTPS: anycast, so near on any network, and
/// always listening. An address, so no DNS lookup adds to the time.
const TARGET: SocketAddr = SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(1, 1, 1, 1), 443));
/// Long enough for two SYN retransmissions on a poor link.
const TIMEOUT: Duration = Duration::from_secs(4);
/// Probes counted for loss and the latency range: five minutes of them.
const WINDOW: usize = 20;
/// How long a probe counts: [`WINDOW`] probes' worth, five minutes.
const SPAN: Duration = Duration::from_secs(WINDOW as u64 * INTERVAL.as_secs());
/// Latency shown is the median of this many recent probes, about a minute,
/// so one slow handshake does not color the bar.
const LATENCY_PROBES: usize = 4;
/// How long the worker outlives the last window that asked for it. Windows
/// reload settings separately, so one can still hold the old value while
/// another has the new one; this keeps a stale `false` from stopping probes
/// another window wants, while one that wants them ticks every 16 ms.
const RELEASE: Duration = Duration::from_secs(1);

struct Worker {
    stop: Arc<Stop>,
    results: mpsc::Receiver<Outcome>,
}

impl Drop for Worker {
    /// The thread ends after its current probe; nothing waits for it here.
    fn drop(&mut self) {
        self.stop.stop();
    }
}

fn spawn() -> Option<Worker> {
    let stop = Arc::new(Stop::default());
    let (sender, results) = mpsc::sync_channel(4);
    let shared = stop.clone();
    let spawned = thread::Builder::new()
        .name("herdr-network-quality".into())
        .spawn(move || run(&shared, &sender));
    match spawned {
        Ok(_) => Some(Worker { stop, results }),
        Err(error) => {
            tracing::warn!(category = "network-quality", %error, "could not start a latency worker");
            None
        }
    }
}

fn run(stop: &Stop, results: &mpsc::SyncSender<Outcome>) {
    loop {
        let due = Instant::now() + INTERVAL;
        let outcome = probe::run(TARGET, TIMEOUT);
        match results.try_send(outcome) {
            // The UI is behind; it will take the next one.
            Ok(()) | Err(mpsc::TrySendError::Full(_)) => {}
            Err(mpsc::TrySendError::Disconnected(_)) => return,
        }
        if !stop.wait(due) {
            return;
        }
    }
}

/// Recent probes, oldest first: at most [`WINDOW`], none older than
/// [`SPAN`]. Each carries the wall-clock time it arrived, which, unlike
/// `Instant` on macOS, keeps counting while the machine sleeps, so the
/// probes from before a night's sleep expire on waking.
#[derive(Default)]
pub(crate) struct Reading {
    recent: VecDeque<(SystemTime, Outcome)>,
}

impl Reading {
    pub fn latest(&self) -> Option<Outcome> {
        self.recent.back().map(|(_, outcome)| *outcome)
    }

    pub fn probes(&self) -> usize {
        self.recent.len()
    }

    fn outcomes(&self) -> impl DoubleEndedIterator<Item = Outcome> + Clone + '_ {
        self.recent.iter().map(|(_, outcome)| *outcome)
    }

    /// The median round trip of the last [`LATENCY_PROBES`] probes that were
    /// answered; None when none of them was.
    pub fn latency(&self) -> Option<Duration> {
        let mut answered: Vec<Duration> = self
            .outcomes()
            .rev()
            .take(LATENCY_PROBES)
            .filter_map(Outcome::round_trip)
            .collect();
        answered.sort_unstable();
        answered.get(answered.len() / 2).copied()
    }

    /// The fastest and slowest answered probes in the window.
    pub fn range(&self) -> Option<(Duration, Duration)> {
        let answered = self.outcomes().filter_map(Outcome::round_trip);
        let min = answered.clone().min()?;
        let max = answered.max()?;
        Some((min, max))
    }

    /// Unanswered probes as a share of the window, 0..=100.
    pub fn loss_percent(&self) -> Option<f32> {
        if self.recent.is_empty() {
            return None;
        }
        let lost = self
            .outcomes()
            .filter(|outcome| outcome.round_trip().is_none())
            .count();
        Some(lost as f32 / self.recent.len() as f32 * 100.)
    }

    fn apply(&mut self, at: SystemTime, outcome: Outcome) {
        if self.recent.len() == WINDOW {
            self.recent.pop_front();
        }
        self.recent.push_back((at, outcome));
    }

    /// Drops probes older than [`SPAN`] at `now`, and any stamped after
    /// `now`, which only a clock set back produces. Whether any went.
    fn expire(&mut self, now: SystemTime) -> bool {
        let before = self.recent.len();
        self.recent
            .retain(|(at, _)| now.duration_since(*at).is_ok_and(|age| age <= SPAN));
        self.recent.len() != before
    }
}

/// One worker for the whole app: every window shows the same machine's
/// link, so more windows must not mean more probes.
#[derive(Default)]
pub(crate) struct NetworkQuality {
    /// None while hidden, or once its thread could not start or has ended.
    worker: Option<Worker>,
    /// A worker that could not start is not retried on every tick.
    started: bool,
    /// When a window last asked for probes.
    wanted: Option<Instant>,
    reading: Reading,
    /// Counts changes to what is shown, so each window can tell on its own
    /// tick whether to redraw after another window took the probes.
    revision: u64,
}

impl Global for NetworkQuality {}

/// The shared reading, if probing has started.
pub(crate) fn reading(cx: &App) -> Option<&Reading> {
    cx.try_global::<NetworkQuality>()
        .map(|quality| &quality.reading)
}

/// Takes finished probes while any window has them `enabled`, or stops and
/// forgets them once none has for [`RELEASE`], and returns the revision of
/// what is shown.
pub(crate) fn poll(enabled: bool, cx: &mut App) -> u64 {
    let quality = cx.default_global::<NetworkQuality>();
    if quality.poll(enabled, Instant::now(), SystemTime::now()) {
        quality.revision += 1;
    }
    quality.revision
}

impl NetworkQuality {
    /// Probes while some window asked within [`RELEASE`] of `tick`, and
    /// stops and forgets the readings once none has. `now` stamps and
    /// expires probes. Returns whether anything shown changed.
    fn poll(&mut self, enabled: bool, tick: Instant, now: SystemTime) -> bool {
        if enabled {
            self.wanted = Some(tick);
        }
        let wanted = self
            .wanted
            .is_some_and(|at| tick.saturating_duration_since(at) < RELEASE);
        if !wanted {
            let had_any = self.started;
            self.worker = None;
            self.started = false;
            self.wanted = None;
            self.reading = Reading::default();
            return had_any;
        }
        if !self.started {
            self.started = true;
            self.worker = spawn();
        }
        let mut changed = self.reading.expire(now);
        while let Some(worker) = &self.worker {
            match worker.results.try_recv() {
                Ok(outcome) => {
                    self.reading.apply(now, outcome);
                    changed = true;
                }
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => self.worker = None,
            }
        }
        changed
    }
}
