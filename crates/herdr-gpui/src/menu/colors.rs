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

/// Lift the button above both the normal hover row and the current-session tint.
pub(super) fn action_hover(theme: &crate::config::Theme) -> Rgba {
    rgb(theme.active).blend(rgba((theme.foreground << 8) | 0x60))
}

/// The connected-or-running indicator the device picker and the session list
/// both green, so one machine's state reads the same in each.
pub(super) const ONLINE: u32 = 0x63c68b;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn action_hover_stays_distinct_from_highlighted_rows() {
        for name in crate::config::Theme::BUILTIN_NAMES {
            let theme = crate::config::Theme::builtin(name)
                .unwrap_or_else(|| panic!("missing theme {name}"));
            let hover = action_hover(&theme);
            let distance = |other: Rgba| {
                (hover.r - other.r).abs() + (hover.g - other.g).abs() + (hover.b - other.b).abs()
            };
            for background in [theme.surface, theme.active, theme.primary_wash()] {
                assert!(
                    distance(rgb(background)) > 0.12,
                    "{name}: button disappears into row"
                );
            }
            assert!(
                distance(rgb(theme.foreground)) > 0.9,
                "{name}: icon lacks contrast"
            );
        }
    }
}
