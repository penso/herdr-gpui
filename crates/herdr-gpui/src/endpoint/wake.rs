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
//! network path monitor says when a usable path comes back, which recovers
//! connections as a wake does, and when a usable path changes, such as a VPN
//! coming up or the primary interface moving, which only has live links
//! prove themselves: such reports also arrive for changes that matter to no
//! connection, so they must not abandon dials or start backoffs over.
//! Elsewhere only the clocks are watched.
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

/// What made connections suspect, weakest first: a stronger trigger does
/// everything a weaker one does.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Trigger {
    /// A usable path changed into another usable one.
    PathChanged,
    /// A usable path came back after there was none.
    NetworkRestored,
    Sleep,
}

/// How many network changes the monitor has reported, by kind.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct NetworkChanges {
    pub(crate) restored: u64,
    pub(crate) changed: u64,
}

pub(crate) struct WakeClock {
    instant: Instant,
    wall: SystemTime,
    /// The network changes reported at the last reading.
    network: NetworkChanges,
    /// The strongest network change seen but not yet recovered from, held
    /// back by `NETWORK_SETTLE`.
    pending: Option<Trigger>,
    recovered: Option<Instant>,
}

impl WakeClock {
    pub(crate) fn new(instant: Instant, wall: SystemTime, network: NetworkChanges) -> Self {
        Self {
            instant,
            wall,
            network,
            pending: None,
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
    /// network changes reported so far. A sleep always recovers: the
    /// monotonic clock barely moved across it, so no settling gap can be
    /// measured against it.
    pub(crate) fn due(
        &mut self,
        instant: Instant,
        wall: SystemTime,
        network: NetworkChanges,
    ) -> Option<Trigger> {
        let slept = self.woke(instant, wall);
        let seen = if network.restored != self.network.restored {
            Some(Trigger::NetworkRestored)
        } else if network.changed != self.network.changed {
            Some(Trigger::PathChanged)
        } else {
            None
        };
        self.pending = self.pending.max(seen);
        self.network = network;
        let trigger = if slept {
            Trigger::Sleep
        } else if let Some(pending) = self.pending
            && self
                .recovered
                .is_none_or(|at| instant.saturating_duration_since(at) >= NETWORK_SETTLE)
        {
            pending
        } else {
            return None;
        };
        self.pending = None;
        self.recovered = Some(instant);
        Some(trigger)
    }
}

/// Starts watching the network, once for the app rather than per window, so
/// test windows never start the system's monitor. Does nothing where no
/// monitor is available.
pub(crate) fn watch_network() {
    #[cfg(target_os = "macos")]
    network::watch();
}

/// The network changes reported since [`watch_network`]; always none before
/// it or where none is watched.
pub(crate) fn network_changes() -> NetworkChanges {
    #[cfg(target_os = "macos")]
    {
        network::changes()
    }
    #[cfg(not(target_os = "macos"))]
    {
        NetworkChanges::default()
    }
}

impl HerdrWindow {
    /// After a sleep or a network change, have every endpoint recover rather
    /// than wait out its backoff: the network the user woke to, or just
    /// joined, may be the one it was waiting for. Each endpoint recovers in
    /// its own poll, once it has taken what its worker reported, so a
    /// recovery never acts on a status one update old.
    pub(crate) fn recover_connections(
        &mut self,
        instant: Instant,
        wall: SystemTime,
        network: NetworkChanges,
    ) -> bool {
        let Some(trigger) = self.wake.due(instant, wall, network) else {
            return false;
        };
        match trigger {
            Trigger::Sleep => {
                tracing::info!(category = "wake", "machine woke; checking connections");
            }
            Trigger::NetworkRestored => {
                tracing::info!(
                    category = "network",
                    "network came back; checking connections"
                );
            }
            Trigger::PathChanged => {
                tracing::info!(
                    category = "network",
                    "network path changed; checking live connections"
                );
            }
        }
        for endpoint in &mut self.endpoints {
            endpoint.recovery = endpoint.recovery.max(Some(trigger));
        }
        true
    }
}

#[cfg(test)]
mod tests;
