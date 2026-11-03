//! Bounds how often a mod's `view` runs, whatever wakes it.

use std::time::{Duration, Instant};

/// At most one run per `gap`. The caller passes the time, so tests and the
/// worker agree on it without a clock of their own.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Pace {
    gap: Duration,
    last: Option<Instant>,
}

impl Pace {
    pub fn new(gap: Duration) -> Self {
        Self { gap, last: None }
    }

    /// How long until a run is allowed; zero when one is allowed now.
    pub fn wait(&self, now: Instant) -> Duration {
        self.last
            .map(|last| (last + self.gap).saturating_duration_since(now))
            .unwrap_or_default()
    }

    /// Records a run at `now`, if one is allowed; returns whether it was.
    pub fn take(&mut self, now: Instant) -> bool {
        if !self.wait(now).is_zero() {
            return false;
        }
        self.last = Some(now);
        true
    }
}
