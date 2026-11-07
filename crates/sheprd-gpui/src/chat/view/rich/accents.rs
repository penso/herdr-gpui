//! The theme's colors in a message's words. Each keeps its hue and moves
//! only as far as text contrast needs on what the words sit on: the message's
//! ground, a table's rows, or a quote's tint.

use crate::config::Theme;

/// The WCAG minimum for text.
const TEXT_CONTRAST: f32 = 4.5;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Accents {
    pub(super) emphasis: u32,
    pub(super) heading: u32,
    pub(super) link: u32,
}

impl Accents {
    /// The accents of a message drawn on `ground`.
    pub(super) fn on(theme: &Theme, ground: u32) -> Self {
        let ink = |color| {
            crate::contrast::ink_on_chrome(
                color,
                [ground, theme.background, theme.surface],
                TEXT_CONTRAST,
            )
        };
        Self {
            emphasis: ink(theme.palette[6]),
            heading: ink(theme.palette[4]),
            link: ink(theme.palette[4]),
        }
    }
}

/// Inline code's ink. It owns its background, so it reads the same in every
/// message, a colored bubble included.
pub(super) fn code_ink(theme: &Theme) -> u32 {
    super::super::text_ink(theme.palette[5], theme.active)
}
