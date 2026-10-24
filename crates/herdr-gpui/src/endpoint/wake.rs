//! Recovering connections after the machine sleeps or its network changes. A
//! link that died meanwhile is otherwise found only by the routine health
//! check of a quiet link, and a dropped one waits out a backoff earned while
//! no network could have answered.
//!
//! Sleep is told from the two clocks: the monotonic clock behind `Instant`
//! stops while macOS and Linux are suspended, and the wall clock does not. A
//! wall clock set forward looks the same, and costs only one extra ping.
//! Whether Windows' monotonic clock stops in sleep is unproven; SSH endpoints,
//! the links this matters for, are not supported there.
//!
//! The network after a wake is often not up yet: Wi-Fi joins seconds later,
//! and a dial made before then hangs until its timeout. On macOS the system's
//! network path monitor says when a usable path appears or changes, and that
//! recovers connections again. Elsewhere only the clocks are watched.
use crate::HerdrWindow;
use std::time::{Duration, Instant, SystemTime};

#[cfg(target_os = "macos")]
mod network;

/// How far the wall clock must run ahead of the monotonic one to count as a
/// sleep. Long enough that scheduling jitter and small clock slews never do.
const SLEEP_GAP: Duration = Duration::from_secs(5);
/// The fewest seconds between recoveries a network change starts. A path
/// settling reports several changes in a burst; the first recovers at once,
/// and the rest wait out this gap and recover once more for the final path.
const NETWORK_SETTLE: Duration = Duration::from_secs(1);

/// What made connections suspect.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Trigger {
    Sleep,
    Network,
}

pub(crate) struct WakeClock {
    instant: Instant,
    wall: SystemTime,
    /// How many network changes had been reported at the last reading.
    network: u64,
    /// A network change seen but not yet recovered from, held back by
    /// `NETWORK_SETTLE`.
    pending: bool,
    recovered: Option<Instant>,
}

impl WakeClock {
    pub(crate) fn new(instant: Instant, wall: SystemTime, network: u64) -> Self {
        Self {
            instant,
            wall,
            network,
            pending: false,
            recovered: None,
        }
    }

    pub(crate) fn now() -> Self {
        Self::new(Instant::now(), SystemTime::now(), network_changes())
    }

    /// Whether the machine slept since the last reading. A wall clock set
    /// backwards is no sleep.
    pub(crate) fn woke(&mut self, instant: Instant, wall: SystemTime) -> bool {
        let awake = instant.saturating_duration_since(self.instant);
        let passed = wall.duration_since(self.wall).unwrap_or_default();
        self.instant = instant;
        self.wall = wall;
        passed.saturating_sub(awake) >= SLEEP_GAP
    }

    /// Whether connections should be recovered now, and why. `network` is the
    /// count of network changes reported so far. A sleep always recovers: the
    /// monotonic clock barely moved across it, so no settling gap can be
    /// measured against it.
    pub(crate) fn due(
        &mut self,
        instant: Instant,
        wall: SystemTime,
        network: u64,
    ) -> Option<Trigger> {
        let slept = self.woke(instant, wall);
        self.pending |= network != self.network;
        self.network = network;
        let trigger = if slept {
            Trigger::Sleep
        } else if self.pending
            && self
                .recovered
                .is_none_or(|at| instant.saturating_duration_since(at) >= NETWORK_SETTLE)
        {
            Trigger::Network
        } else {
            return None;
        };
        self.pending = false;
        self.recovered = Some(instant);
        Some(trigger)
    }
}

/// How many times a usable network path has appeared or changed since the
/// monitor started; always zero where none is watched.
fn network_changes() -> u64 {
    #[cfg(target_os = "macos")]
    {
        network::changes()
    }
    #[cfg(not(target_os = "macos"))]
    {
        0
    }
}

impl HerdrWindow {
    /// After a sleep or a network change, recover every endpoint at once
    /// rather than at the end of its backoff: the network the user woke to,
    /// or just joined, may be the one it was waiting for.
    pub(crate) fn recover_connections(&mut self, instant: Instant, wall: SystemTime) -> bool {
        let Some(trigger) = self.wake.due(instant, wall, network_changes()) else {
            return false;
        };
        match trigger {
            Trigger::Sleep => {
                tracing::info!(category = "wake", "machine woke; checking connections");
            }
            Trigger::Network => {
                tracing::info!(
                    category = "network",
                    "network changed; checking connections"
                );
            }
        }
        for endpoint in &mut self.endpoints {
            endpoint.recover(instant);
        }
        true
    }
}

#[cfg(test)]
mod tests;
