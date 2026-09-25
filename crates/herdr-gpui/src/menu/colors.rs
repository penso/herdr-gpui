//! Accent and danger colors derived from the active theme's ANSI palette,
//! mixed with foreground so they stay readable on dark surfaces.

use gpui::{Rgba, rgb, rgba};

/// Mix an ANSI color with foreground so it stays readable on dark themes, where
/// the palette entry alone can sit too close to the surface it is painted on.
fn tint(theme: &crate::config::Theme, index: usize) -> Rgba {
    rgb(theme.foreground).blend(rgba((theme.palette[index] << 8) | 0x70))
}

/// The theme's blue, as accents and links use it.
pub(crate) fn accent(theme: &crate::config::Theme) -> Rgba {
    tint(theme, 4)
}

/// The theme's red, as destructive actions and errors use it.
pub(super) fn danger(theme: &crate::config::Theme) -> Rgba {
    tint(theme, 1)
}

/// The connected-or-running indicator the device picker and the session list
/// both green, so one machine's state reads the same in each.
pub(super) const ONLINE: u32 = 0x63c68b;
