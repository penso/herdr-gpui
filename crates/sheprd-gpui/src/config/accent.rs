//! `accent`: whether the app's own accent color — selected tabs, rows, and
//! focus marks, [`super::Theme::primary`] — comes from the theme or from the
//! system. Terminal colors never change with it.
use serde::Deserialize;

#[cfg(windows)]
mod windows;

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum Accent {
    /// The theme's own accent: a desktop theme's or Herdr's `accent`, else
    /// its palette's magenta.
    #[default]
    Theme,
    /// The accent color chosen in Windows Settings > Personalization >
    /// Colors. Other platforms have no such lookup yet and keep the theme's.
    System,
}

impl Accent {
    /// The color to use in place of the theme's accent, if any. Reads the
    /// system setting each time, so a theme loaded after the user changed it
    /// picks up the new color.
    pub(crate) fn color(self) -> Option<u32> {
        match self {
            Self::Theme => None,
            #[cfg(windows)]
            Self::System => windows::accent(),
            #[cfg(not(windows))]
            Self::System => None,
        }
    }
}
