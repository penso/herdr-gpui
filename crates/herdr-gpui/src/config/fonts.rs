//! Font faces, their configured overrides and sizes, and the icon-font cascade.
use super::Config;
use crate::{Error, Result};
use gpui::{Font, FontFallbacks};
use serde::Deserialize;
use std::ops::RangeInclusive;

/// Every face is held to this range, whether it comes from the config file or
/// from a runtime adjustment, so the two can never disagree on what is valid.
pub const FONT_SIZE_RANGE: RangeInclusive<f32> = 8.0..=48.0;

/// One logical pixel: the smallest step that can move the terminal cell grid.
pub const FONT_SIZE_STEP: f32 = 1.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FontFace {
    Sidebar,
    Tabs,
    Terminal,
    Ui,
}

impl FontFace {
    pub(crate) fn set_size(self, config: &mut Config, size: f32) {
        match self {
            Self::Sidebar => config.sidebar.size = size,
            Self::Tabs => config.tabs.size = size,
            Self::Terminal => config.terminal.size = size,
            Self::Ui => config.ui.size = size,
        }
    }

    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Sidebar => "sidebar",
            Self::Tabs => "tabs",
            Self::Terminal => "terminal",
            Self::Ui => "ui",
        }
    }

    pub(crate) fn size(self, config: &Config) -> f32 {
        match self {
            Self::Sidebar => config.sidebar.size,
            Self::Tabs => config.tabs.size,
            Self::Terminal => config.terminal.size,
            Self::Ui => config.ui.size,
        }
    }
}

/// Upper bound on a configured cascade. Every entry is searched for each
/// uncovered codepoint, so a long list costs shaping time and covers nothing a
/// short one does not. Names that are not installed are ignored by the platform.
pub(super) const MAX_FONT_FALLBACKS: usize = 8;

#[derive(Default, Deserialize)]
#[serde(default)]
pub(super) struct FontSettings {
    family: Option<String>,
    size: Option<f32>,
    fallback: Option<Vec<String>>,
}

impl FontSettings {
    /// Merges this face's configured keys over `font`, then validates the result.
    pub(super) fn apply(self, name: &'static str, font: &mut FontConfig) -> Result<()> {
        if let Some(family) = self.family {
            font.family = family;
        }
        if let Some(size) = self.size {
            font.size = size;
        }
        if let Some(fallback) = self.fallback {
            if fallback.len() > MAX_FONT_FALLBACKS {
                return Err(Error::TooManyFontFallbacks(name));
            }
            if fallback.iter().any(|family| family.trim().is_empty()) {
                return Err(Error::EmptyFontFallback(name));
            }
            font.fallbacks = Some(fallback);
        }
        if font.family.trim().is_empty() {
            return Err(Error::EmptyFontFamily(name));
        }
        if !font.size.is_finite() || !FONT_SIZE_RANGE.contains(&font.size) {
            return Err(Error::InvalidFontSize(name));
        }
        Ok(())
    }
}

/// Nerd Font patches keep this marker in every patched family name, so matching
/// it finds the installed icon faces without naming individual fonts.
const SYMBOL_FAMILY_MARKER: &str = "nerd font";

/// A cascade is searched in order for every uncovered codepoint, so automatic
/// detection keeps only the best-ranked few families.
const MAX_DETECTED_FALLBACKS: usize = 3;

#[derive(Clone, Debug)]
pub struct FontConfig {
    pub family: String,
    pub size: f32,
    /// Families searched, nearest first, for glyphs `family` lacks. `None`
    /// until the config names them or [`Config::resolve_font_fallbacks`]
    /// detects them; an empty list opts out of any cascade.
    pub fallbacks: Option<Vec<String>>,
}

impl FontConfig {
    pub fn line_height(&self) -> f32 {
        self.size * 20.0 / 14.0
    }

    /// The shaping font for this face. Terminal prompts draw powerline
    /// separators and Nerd Font icons from the Private Use Area, which no text
    /// face and no platform default cascade covers, so those cells shape to the
    /// missing-glyph box unless the cascade names an icon font explicitly.
    pub fn font(&self) -> Font {
        let mut font = gpui::font(self.family.clone());
        font.fallbacks = self
            .fallbacks
            .as_ref()
            .filter(|families| !families.is_empty())
            .map(|families| FontFallbacks::from_fonts(families.clone()));
        font
    }
}

/// Ranks an installed Nerd Font family for the automatic cascade. Symbols-only
/// faces carry the icon ranges without replacing any text glyph, and `Mono`
/// variants keep every icon inside a single terminal cell, so both come first.
fn fallback_rank(family: &str) -> u8 {
    let lowercase = family.to_lowercase();
    let symbols = lowercase.starts_with("symbols nerd font");
    let mono = lowercase.ends_with(" mono");
    match (symbols, mono) {
        (true, true) => 0,
        (true, false) => 1,
        (false, true) => 2,
        (false, false) => 3,
    }
}

/// Picks the installed icon families to search for Private Use Area glyphs.
/// Ranking then alphabetical order keeps one machine's font set mapping to one
/// cascade, so a rendering report describes a reproducible configuration.
pub fn symbol_fallbacks(installed: impl IntoIterator<Item = String>) -> Vec<String> {
    let mut families: Vec<String> = installed
        .into_iter()
        .filter(|family| family.to_lowercase().contains(SYMBOL_FAMILY_MARKER))
        .collect();
    families.sort_unstable();
    families.dedup();
    families.sort_by_key(|family| fallback_rank(family));
    families.truncate(MAX_DETECTED_FALLBACKS);
    families
}

impl Config {
    /// Gives every face the config left alone an automatic icon-font cascade.
    /// `installed` is consulted only when some face still needs one, because
    /// enumerating system fonts is slow enough to keep off the UI thread.
    pub fn resolve_font_fallbacks<I>(&mut self, installed: impl FnOnce() -> I)
    where
        I: IntoIterator<Item = String>,
    {
        let faces = [
            &mut self.sidebar,
            &mut self.tabs,
            &mut self.terminal,
            &mut self.ui,
        ];
        if faces.iter().all(|face| face.fallbacks.is_some()) {
            return;
        }
        let detected = symbol_fallbacks(installed());
        for face in faces {
            if face.fallbacks.is_none() {
                face.fallbacks = Some(detected.clone());
            }
        }
    }
}
