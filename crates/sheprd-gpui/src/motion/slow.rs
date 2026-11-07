//! QA > Slow Animations: every animation in the window at a tenth of its
//! speed, to watch how it moves. Animations read the time from [`now`]
//! rather than [`Instant::now`], and GPUI's own animations take their
//! length through [`duration`]. While the switch is on, `now` advances a
//! tenth as fast from the moment it was turned on; turning it off returns
//! to the real time, so whatever is mid-way finishes at once.
//!
//! The switch is process-wide, like the app's menu bar that holds it, and
//! reaches places that draw without an app context.

use std::{
    sync::Mutex,
    time::{Duration, Instant},
};

/// How many times slower animations run while the switch is on.
const FACTOR: u32 = 10;

/// When slow motion was turned on, while it is.
static SINCE: Mutex<Option<Instant>> = Mutex::new(None);

fn since() -> Option<Instant> {
    SINCE.lock().map_or(None, |since| *since)
}

/// Turns slow motion on or off.
pub(crate) fn set_slow(slow: bool) {
    if let Ok(mut since) = SINCE.lock() {
        *since = slow.then(Instant::now);
    }
}

/// Whether animations run slowly.
pub(crate) fn is_slow() -> bool {
    since().is_some()
}

/// The time animations read: the real time, or while slow motion is on,
/// time that has passed a tenth as fast since it was turned on.
pub(crate) fn now() -> Instant {
    at(Instant::now(), since())
}

/// `length` as an animation GPUI times itself should last.
pub(crate) fn duration(length: Duration) -> Duration {
    stretch(length, is_slow())
}

fn at(real: Instant, since: Option<Instant>) -> Instant {
    match since {
        Some(since) => since + real.saturating_duration_since(since) / FACTOR,
        None => real,
    }
}

fn stretch(length: Duration, slow: bool) -> Duration {
    if slow { length * FACTOR } else { length }
}

#[cfg(test)]
mod tests;
