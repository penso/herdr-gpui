//! Applying a configured face to elements.
//!
//! Shell prompts draw powerline separators and Nerd Font icons from the Private
//! Use Area. No text face covers those codepoints and no platform default
//! cascade reaches an installed icon font, so every such cell shapes to the
//! missing-glyph box until the cascade names one. [`crate::config::FontConfig`]
//! carries that cascade alongside the family; this trait applies both.

use crate::config::FontConfig;
use gpui_kit::{Font, Styled};

pub(crate) trait StyledFont: Styled + Sized {
    /// Sets the family and its fallback cascade, and nothing else. `Styled::font`
    /// would also reset weight and style, which nested chrome inherits.
    fn text_font(mut self, config: &FontConfig) -> Self {
        let Font {
            family, fallbacks, ..
        } = config.font();
        let style = &mut self.style().text;
        style.font_family = Some(family);
        style.font_fallbacks = fallbacks;
        self
    }
}

impl<E: Styled> StyledFont for E {}
