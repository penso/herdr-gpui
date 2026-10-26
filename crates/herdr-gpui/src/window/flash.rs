//! The brief message over the terminal: "copied to clipboard", why a
//! shortcut did nothing, or how to do what a gesture did not. It answers the
//! user's own gesture, so it needs no dismissing and retires on its own. Any
//! window code can raise one with [`HerdrWindow::show_flash`]; nothing here
//! names individual messages. It fades in where it shows and fades back out
//! when its time is up.

use super::HerdrWindow;
use crate::{
    config::Theme,
    motion::{self, ENTER, LEAVE},
};
use gpui::{Context, SharedString};
use std::time::{Duration, Instant};

/// How long a flash stays up, matching herdr's own clipboard feedback.
const FLASH_DURATION: Duration = Duration::from_secs(2);

/// How long a hint stays up. It teaches rather than confirms, so it needs a
/// read, not a glance.
const HINT_DURATION: Duration = Duration::from_secs(4);

/// How far the flash over the terminal slides in from the edge it sits by.
pub(super) const SLIDE: f32 = 6.;

/// Whether the flash reports something done or something declined, or
/// suggests another way to do what was tried.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Tone {
    Hint,
    Success,
    Warning,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Flash {
    pub(crate) tone: Tone,
    pub(crate) text: SharedString,
}

impl Flash {
    pub(crate) fn success(text: impl Into<SharedString>) -> Self {
        Self {
            tone: Tone::Success,
            text: text.into(),
        }
    }

    pub(crate) fn warning(text: impl Into<SharedString>) -> Self {
        Self {
            tone: Tone::Warning,
            text: text.into(),
        }
    }

    pub(crate) fn hint(text: impl Into<SharedString>) -> Self {
        Self {
            tone: Tone::Hint,
            text: text.into(),
        }
    }

    /// Green for something done, yellow for something declined, blue for a
    /// suggestion, as the window's other notices use it.
    pub(crate) fn accent(&self, theme: &Theme) -> u32 {
        theme.ink(match self.tone {
            Tone::Hint => theme.palette[4],
            Tone::Success => theme.palette[2],
            Tone::Warning => theme.palette[3],
        })
    }

    fn duration(&self) -> Duration {
        match self.tone {
            Tone::Hint => HINT_DURATION,
            Tone::Success | Tone::Warning => FLASH_DURATION,
        }
    }
}

/// How the flash comes and goes, timed by [`motion::now`] rather than the
/// real time its expiry keeps.
#[derive(Debug, Default)]
pub(crate) struct Motion {
    /// When the flash up now began to fade in.
    since: Option<Instant>,
    /// The flash that retired, fading out from when it did.
    leaving: Option<(Flash, Instant)>,
}

impl Motion {
    fn moving(&self, now: Instant) -> bool {
        self.since
            .is_some_and(|since| motion::progress(since, now, ENTER).is_some())
            || self
                .leaving
                .as_ref()
                .is_some_and(|(_, since)| motion::progress(*since, now, LEAVE).is_some())
    }
}

impl HerdrWindow {
    /// Replaces whatever flash is up, so the newest gesture is the one answered.
    /// A flash already up takes the new text where it is, without fading again.
    pub(crate) fn show_flash(&mut self, flash: Flash, cx: &mut Context<Self>) {
        let expires = Instant::now() + flash.duration();
        if self.flash.is_none() {
            self.flash_motion.since = Some(motion::now());
        }
        self.flash_motion.leaving = None;
        self.flash = Some((flash, expires));
        cx.notify();
    }

    /// The flash to draw and how much of it shows, from 0 to 1: the one up
    /// now as it fades in, or the one that just retired as it fades out.
    pub(crate) fn flash_shown(&self) -> Option<(&Flash, f32)> {
        let now = motion::now();
        if let Some((flash, _)) = &self.flash {
            let shown = self
                .flash_motion
                .since
                .and_then(|since| motion::progress(since, now, ENTER))
                .unwrap_or(1.);
            return Some((flash, shown));
        }
        let (flash, since) = self.flash_motion.leaving.as_ref()?;
        motion::progress(*since, now, LEAVE).map(|k| (flash, 1. - k))
    }

    /// Whether the flash is still fading in or out.
    pub(crate) fn flash_moving(&self) -> bool {
        self.flash_motion.moving(motion::now())
    }

    /// Retires the flash once its time is up. `true` when the window has to
    /// repaint without it.
    pub(crate) fn tick_flash(&mut self, now: Instant) -> bool {
        if self
            .flash
            .as_ref()
            .is_none_or(|(_, expires)| now < *expires)
        {
            return false;
        }
        if let Some((flash, _)) = self.flash.take() {
            self.flash_motion.leaving = Some((flash, motion::now()));
        }
        true
    }
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests;
