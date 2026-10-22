//! The brief message over the terminal: "copied to clipboard", why a
//! shortcut did nothing, or how to do what a gesture did not. It answers the
//! user's own gesture, so it needs no dismissing and retires on its own. Any
//! window code can raise one with [`HerdrWindow::show_flash`]; nothing here
//! names individual messages.

use super::HerdrWindow;
use crate::config::Theme;
use gpui::{Context, SharedString};
use std::time::{Duration, Instant};

/// How long a flash stays up, matching herdr's own clipboard feedback.
const FLASH_DURATION: Duration = Duration::from_secs(2);

/// How long a hint stays up. It teaches rather than confirms, so it needs a
/// read, not a glance.
const HINT_DURATION: Duration = Duration::from_secs(4);

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

impl HerdrWindow {
    /// Replaces whatever flash is up, so the newest gesture is the one answered.
    pub(crate) fn show_flash(&mut self, flash: Flash, cx: &mut Context<Self>) {
        let expires = Instant::now() + flash.duration();
        self.flash = Some((flash, expires));
        cx.notify();
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
        self.flash = None;
        true
    }
}
