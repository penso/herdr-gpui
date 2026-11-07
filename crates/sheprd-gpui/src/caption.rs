//! The system-drawn caption over a window on Windows: its light or dark
//! colors follow the window's theme rather than the system's app mode, so a
//! dark theme under light Windows does not wear a white title bar.
//!
//! GPUI sets the caption from the system appearance when the window opens
//! and again whenever that changes, so the state is kept against both: a
//! system change is applied over on the next render. Other platforms draw
//! the caption themselves, or under a transparent titlebar, and do nothing.
use crate::config::Theme;
use gpui::{Window, WindowAppearance};

#[cfg(windows)]
pub(crate) mod windows;

/// Whether a theme reads as dark: its text is lighter than its background.
/// A fixed luminance threshold misjudges dim light themes and bright dark ones.
pub(crate) fn dark(theme: &Theme) -> bool {
    crate::contrast::luminance(theme.foreground) > crate::contrast::luminance(theme.background)
}

/// What one window last told the platform about its caption.
#[derive(Debug, Default)]
pub(crate) struct Caption {
    /// The theme's darkness and the system appearance it was applied under.
    applied: Option<(bool, WindowAppearance)>,
}

impl Caption {
    /// Brings the caption in line with `theme`. Cheap when nothing changed:
    /// the platform call happens only when the theme's darkness or the system
    /// appearance did.
    pub(crate) fn sync(&mut self, window: &Window, theme: &Theme) {
        let wanted = (dark(theme), window.appearance());
        if self.applied == Some(wanted) {
            return;
        }
        self.applied = Some(wanted);
        #[cfg(windows)]
        windows::dark_caption(window, wanted.0);
    }
}

#[cfg(test)]
mod tests;
