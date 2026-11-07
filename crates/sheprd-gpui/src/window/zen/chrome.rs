//! The timeline of the chrome zen mode slides away and back. Entering, the
//! status bar leaves first and the sidebar follows once it has gone, each
//! easing in and out, so the window clears in two calm steps. Leaving, both
//! come back together, quicker, easing out into place.
//!
//! Unlike [`crate::motion`], a slide here runs on a [`Clock`] the window
//! moves on as it draws, so a slow frame can delay it but never skip it.

use std::time::{Duration, Instant};

/// How long the status bar takes to slide off on the way in.
pub(super) const HIDE_BAR: Duration = Duration::from_millis(220);
/// How long the sidebar then takes; it travels further.
pub(super) const HIDE_SIDEBAR: Duration = Duration::from_millis(280);
/// How long both take to come back on the way out.
pub(super) const SHOW: Duration = crate::motion::ENTER;

/// How much of each part shows, from 0 gone to 1 in place.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Shown {
    pub(crate) bar: f32,
    pub(crate) sidebar: f32,
}

impl Shown {
    const ALL: Self = Self {
        bar: 1.,
        sidebar: 1.,
    };
    const NONE: Self = Self {
        bar: 0.,
        sidebar: 0.,
    };
}

/// How far a slide has run, counted in drawn frames rather than read off
/// the wall clock. Entering zen mode lays the window out afresh, and the
/// frames that does takes would otherwise eat the start of the slide; here
/// the first frame shows its start, and no frame moves it on by more than
/// [`MAX_STEP`].
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Clock {
    elapsed: Duration,
    last: Option<Instant>,
}

/// The most one frame moves a slide on: two frames at 60 Hz.
const MAX_STEP: Duration = Duration::from_millis(33);

impl Clock {
    fn tick(&mut self, now: Instant) {
        if let Some(last) = self.last {
            self.elapsed += now.saturating_duration_since(last).min(MAX_STEP);
        }
        self.last = Some(now);
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum Chrome {
    Still(Shown),
    /// Sliding off from `from`, the status bar first.
    Hiding {
        clock: Clock,
        from: Shown,
    },
    /// Sliding back from `from`, both together.
    Showing {
        clock: Clock,
        from: Shown,
    },
}

impl Default for Chrome {
    fn default() -> Self {
        Self::Still(Shown::ALL)
    }
}

/// How far a phase `length` long has come `elapsed` into it, from 0 to 1.
fn phase(elapsed: Duration, length: Duration) -> f32 {
    if length.is_zero() {
        return 1.;
    }
    (elapsed.as_secs_f32() / length.as_secs_f32()).clamp(0., 1.)
}

/// Starts and ends gently, for something leaving of its own accord.
fn ease_in_out(k: f32) -> f32 {
    if k < 0.5 {
        4. * k * k * k
    } else {
        1. - (2. - 2. * k).powi(3) / 2.
    }
}

/// Settles gently into place.
fn ease_out(k: f32) -> f32 {
    1. - (1. - k).powi(3)
}

impl Chrome {
    /// What shows now, and whether it is still moving.
    pub(crate) fn at(&self) -> (Shown, bool) {
        match *self {
            Self::Still(shown) => (shown, false),
            Self::Hiding { clock, from } => {
                let elapsed = clock.elapsed;
                // A part already partly gone has less to travel, at the
                // same pace.
                let bar = HIDE_BAR.mul_f32(from.bar);
                let sidebar = HIDE_SIDEBAR.mul_f32(from.sidebar);
                let shown = Shown {
                    bar: from.bar * (1. - ease_in_out(phase(elapsed, bar))),
                    sidebar: from.sidebar
                        * (1. - ease_in_out(phase(elapsed.saturating_sub(bar), sidebar))),
                };
                (shown, elapsed < bar + sidebar)
            }
            Self::Showing { clock, from } => {
                let k = ease_out(phase(clock.elapsed, SHOW));
                let shown = Shown {
                    bar: from.bar + (1. - from.bar) * k,
                    sidebar: from.sidebar + (1. - from.sidebar) * k,
                };
                (shown, clock.elapsed < SHOW)
            }
        }
    }

    /// Moves a slide on for a frame drawn at `now`.
    pub(crate) fn tick(&mut self, now: Instant) {
        match self {
            Self::Still(_) => {}
            Self::Hiding { clock, .. } | Self::Showing { clock, .. } => clock.tick(now),
        }
    }

    /// Starts sliding off from wherever it is, or with `animate` off goes
    /// at once.
    pub(crate) fn hide(&mut self, animate: bool) {
        let (from, _) = self.at();
        *self = match animate {
            true => Self::Hiding {
                clock: Clock::default(),
                from,
            },
            false => Self::Still(Shown::NONE),
        };
    }

    /// Starts sliding back from wherever it is, or with `animate` off comes
    /// back at once.
    pub(crate) fn show(&mut self, animate: bool) {
        let (from, _) = self.at();
        *self = match animate {
            true => Self::Showing {
                clock: Clock::default(),
                from,
            },
            false => Self::Still(Shown::ALL),
        };
    }
}

#[cfg(test)]
mod tests;
