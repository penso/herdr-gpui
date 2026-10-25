//! Font faces, their configured overrides and sizes, and the icon-font cascade.
use super::Error;
use super::{
    Config,
    font_weight::{self, MissingWeight},
};
use crate::Result;
use gpui::{Font, FontFallbacks, FontWeight};
use serde::Deserialize;
use std::{collections::BTreeSet, ops::RangeInclusive};

/// Every face is held to this range, whether it comes from the config file or
/// from a runtime adjustment, so the two can never disagree on what is valid.
pub const FONT_SIZE_RANGE: RangeInclusive<f32> = 8.0..=48.0;

/// One logical pixel: the smallest step that can move the terminal cell grid.
pub const FONT_SIZE_STEP: f32 = 1.0;

/// A configured terminal line height. Below 1 rows overlap their neighbors;
/// past 2 most of every row is blank.
pub(super) const LINE_HEIGHT_RANGE: RangeInclusive<f32> = 1.0..=2.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FontFace {
    Sidebar,
    Tabs,
    Terminal,
    Ui,
}

impl FontFace {
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
    /// Untyped because only the terminal takes it: elsewhere it is discarded
    /// as unknown, so its type must not fail the whole file first.
    line_height: Option<toml::Value>,
    /// Untyped for the same reason as `line_height`.
    weight: Option<toml::Value>,
    /// The terminal's padding rather than part of its font, read through
    /// [`Self::take_padding`]. Untyped for the same reason as `line_height`.
    padding_x: Option<toml::Value>,
    padding_y: Option<toml::Value>,
}

impl FontSettings {
    /// Takes the terminal-only keys out of a face that draws no terminal
    /// cells, returning each one present to report as unknown.
    pub(super) fn reject_terminal_keys(&mut self, name: &str) -> Vec<String> {
        [
            ("line_height", self.line_height.take()),
            ("weight", self.weight.take()),
            ("padding_x", self.padding_x.take()),
            ("padding_y", self.padding_y.take()),
        ]
        .into_iter()
        .filter_map(|(key, value)| value.map(|_| format!("{name}.{key}")))
        .collect()
    }

    /// Takes the horizontal and vertical padding keys of the terminal's table.
    pub(super) fn take_padding(&mut self) -> (Option<toml::Value>, Option<toml::Value>) {
        (self.padding_x.take(), self.padding_y.take())
    }

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
                return Err(Error::TooManyFontFallbacks(name).into());
            }
            if fallback.iter().any(|family| family.trim().is_empty()) {
                return Err(Error::EmptyFontFallback(name).into());
            }
            font.fallbacks = Some(fallback);
        }
        if let Some(line_height) = self.line_height {
            let line_height = match line_height {
                toml::Value::Float(value) => value as f32,
                toml::Value::Integer(value) => value as f32,
                _ => return Err(Error::InvalidLineHeight(name).into()),
            };
            if !line_height.is_finite() || !LINE_HEIGHT_RANGE.contains(&line_height) {
                return Err(Error::InvalidLineHeight(name).into());
            }
            font.line_height_multiple = Some(line_height);
        }
        if let Some(weight) = self.weight {
            font.weight = font_weight::parse(weight, name)?;
        }
        if font.family.trim().is_empty() {
            return Err(Error::EmptyFontFamily(name).into());
        }
        if !font.size.is_finite() || !FONT_SIZE_RANGE.contains(&font.size) {
            return Err(Error::InvalidFontSize(name).into());
        }
        Ok(())
    }
}

/// A platform's default families and the installed alternatives that replace
/// them when missing. GPUI shapes a missing family with whatever face the
/// platform finds first, usually proportional, which breaks the cell grid.
#[derive(Clone, Copy, Debug)]
pub(super) struct DefaultFonts {
    pub(super) monospace: &'static str,
    pub(super) sans: &'static str,
    /// Tried in order before any other installed text `Mono` family.
    monospace_alternatives: &'static [&'static str],
    sans_alternatives: &'static [&'static str],
}

/// Linux distributions disagree on which fonts ship by default (Omarchy has no
/// DejaVu), and GPUI bundles none there. JetBrains Mono comes first because a
/// distribution that installs it, like Omarchy, chose it as its terminal face
/// over the Noto fonts it also ships; the Nerd Font build also covers icons.
pub(super) const LINUX_FONTS: DefaultFonts = DefaultFonts {
    monospace: "DejaVu Sans Mono",
    sans: "DejaVu Sans",
    monospace_alternatives: &[
        "JetBrainsMono Nerd Font",
        "JetBrains Mono",
        "Noto Sans Mono",
        "Liberation Mono",
        "Ubuntu Mono",
        "Cascadia Mono",
        "Fira Mono",
        "Source Code Pro",
        "Hack",
    ],
    sans_alternatives: &[
        "Noto Sans",
        "Cantarell",
        "Ubuntu",
        "Liberation Sans",
        "IBM Plex Sans",
    ],
};

/// Cascadia Mono ships with Windows 11 and Windows Terminal; Consolas with
/// every release since Vista. GPUI resolves `.SystemUIFont` to Segoe UI.
pub(super) const WINDOWS_FONTS: DefaultFonts = DefaultFonts {
    monospace: "Cascadia Mono",
    sans: ".SystemUIFont",
    monospace_alternatives: &["Consolas", "Lucida Console", "Courier New"],
    sans_alternatives: &[],
};

/// Menlo and the system UI font ship with every macOS release.
pub(super) const MACOS_FONTS: DefaultFonts = DefaultFonts {
    monospace: "Menlo",
    sans: ".SystemUIFont",
    monospace_alternatives: &[],
    sans_alternatives: &[],
};

pub(super) const PLATFORM_FONTS: DefaultFonts = if cfg!(target_os = "linux") {
    LINUX_FONTS
} else if cfg!(windows) {
    WINDOWS_FONTS
} else {
    MACOS_FONTS
};

/// A text face with fixed-width glyphs, judged by name. Symbols-only and
/// proportional (`Propo`) Nerd Font variants carry no usable text glyphs.
fn is_text_monospace(family: &str) -> bool {
    let lowercase = family.to_lowercase();
    !lowercase.starts_with("symbols")
        && !lowercase.ends_with(" propo")
        && (lowercase.ends_with("mono") || lowercase.contains("mono "))
}

fn first_installed<'a>(candidates: &[&'a str], installed: &BTreeSet<String>) -> Option<&'a str> {
    candidates
        .iter()
        .copied()
        .find(|family| installed.contains(*family))
}

impl DefaultFonts {
    /// Whether `family` is one of these defaults that resolution may replace.
    /// A platform without alternatives always ships its defaults, and
    /// dot-prefixed names are aliases GPUI resolves itself, never listed as
    /// installed, so neither costs a font enumeration.
    pub(super) fn is_replaceable(&self, family: &str) -> bool {
        !self.monospace_alternatives.is_empty()
            && !family.starts_with('.')
            && (family == self.monospace || family == self.sans)
    }

    fn installed_monospace<'a>(&'a self, installed: &'a BTreeSet<String>) -> Option<&'a str> {
        first_installed(self.monospace_alternatives, installed).or_else(|| {
            installed
                .iter()
                .map(String::as_str)
                .find(|family| is_text_monospace(family))
        })
    }

    /// The installed family that replaces a default missing from this
    /// machine, or `None` when `family` is installed or is not a default. The
    /// sans face falls back to monospace text rather than to no face at all.
    pub(super) fn substitute(&self, family: &str, installed: &BTreeSet<String>) -> Option<String> {
        if !self.is_replaceable(family) || installed.contains(family) {
            return None;
        }
        let substitute = if family == self.monospace {
            self.installed_monospace(installed)
        } else {
            first_installed(self.sans_alternatives, installed)
                .or_else(|| self.installed_monospace(installed))
        };
        substitute.map(str::to_owned)
    }
}

/// Nerd Font patches keep this marker in every patched family name, so matching
/// it finds the installed icon faces without naming individual fonts.
const SYMBOL_FAMILY_MARKER: &str = "nerd font";

/// A cascade is searched in order for every uncovered codepoint, so automatic
/// detection keeps only the best-ranked few families.
const MAX_DETECTED_FALLBACKS: usize = 3;

/// Whether `family` is, by its name, a patched face that draws prompt icons
/// itself: a Nerd Font, or one of its abbreviated `NF` builds such as
/// Powerlevel10k's recommended `MesloLGS NF`. Detection looks for
/// [`SYMBOL_FAMILY_MARKER`] and so misses the abbreviated names. A Powerline
/// face is not one: it carries the separators and the branch mark, and none
/// of the icons a Nerd Font prompt draws.
fn draws_icons(family: &str) -> bool {
    let lowercase = family.to_lowercase();
    lowercase.contains(SYMBOL_FAMILY_MARKER)
        || lowercase
            .split_whitespace()
            .any(|word| matches!(word, "nf" | "nfm" | "nfp"))
}

#[derive(Clone, Debug, PartialEq)]
pub struct FontConfig {
    pub family: String,
    pub size: f32,
    /// Families searched, nearest first, for glyphs `family` lacks. `None`
    /// until the config names them or [`Config::resolve_fonts`]
    /// detects them; an empty list opts out of any cascade.
    pub fallbacks: Option<Vec<String>>,
    /// Line height as a multiple of `size`, so it follows runtime font size
    /// changes. `None` keeps the default of 20/14.
    pub line_height_multiple: Option<f32>,
    /// Weight of regular text; bold text draws at [`font_weight::bold`] of it.
    pub weight: FontWeight,
}

impl FontConfig {
    pub fn line_height(&self) -> f32 {
        match self.line_height_multiple {
            Some(multiple) => self.size * multiple,
            // 20 logical pixels at the terminal's default size of 14.
            None => self.size * 20.0 / 14.0,
        }
    }

    /// The shaping font for this face. Terminal prompts draw powerline
    /// separators and Nerd Font icons from the Private Use Area, which no text
    /// face and no platform default cascade covers, so those cells shape to the
    /// missing-glyph box unless the cascade names an icon font explicitly.
    pub fn font(&self) -> Font {
        let mut font = gpui::font(self.family.clone());
        font.weight = self.weight;
        font.fallbacks = self
            .fallbacks
            .as_ref()
            .filter(|families| !families.is_empty())
            .map(|families| FontFallbacks::from_fonts(families.clone()));
        font
    }
}

impl FontConfig {
    /// The families the config named for this face: its own unless it is a
    /// platform default, which resolution substitutes when missing, and every
    /// fallback. Dot-prefixed names are aliases GPUI resolves itself.
    fn requested(&self) -> impl Iterator<Item = &str> {
        let family = (self.family != PLATFORM_FONTS.monospace
            && self.family != PLATFORM_FONTS.sans)
            .then_some(self.family.as_str());
        family
            .into_iter()
            .chain(self.fallbacks.iter().flatten().map(String::as_str))
            .filter(|family| !family.starts_with('.'))
    }
}

/// An unlisted name may still be a legacy alias accepted by the font backend.
fn missing_families<'a>(
    requested: impl IntoIterator<Item = &'a str>,
    installed: &BTreeSet<String>,
    resolves_alias: impl Fn(&str) -> bool,
) -> Vec<String> {
    let installed: BTreeSet<String> = installed
        .iter()
        .map(|family| family.to_lowercase())
        .collect();
    requested
        .into_iter()
        .filter(|family| !installed.contains(&family.to_lowercase()))
        .filter(|family| !resolves_alias(family))
        .map(str::to_owned)
        .collect()
}

fn resolves_platform_alias(family: &str) -> bool {
    #[cfg(target_os = "macos")]
    {
        // Unlike TextSystem::resolve_font, this fails instead of substituting a default.
        font_kit::source::SystemSource::new()
            .select_family_by_name(family)
            .is_ok()
    }
    #[cfg(not(target_os = "macos"))]
    {
        // Preserve enumeration-based validation on the other platforms.
        let _ = family;
        false
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
    /// Gives every face the config left alone an automatic icon-font cascade
    /// and replaces a default family this machine lacks. `installed`
    /// is consulted only when some face still needs it, because enumerating
    /// system fonts is slow enough to keep off the UI thread. Every family
    /// the config names is checked against it and the platform's aliases,
    /// so an uninstalled font is reported without rejecting a valid alias.
    pub fn resolve_fonts<I>(&mut self, installed: impl FnOnce() -> I)
    where
        I: IntoIterator<Item = String>,
    {
        self.resolve_fonts_with(installed, resolves_platform_alias);
        self.check_font_weight(font_weight::platform_has_weight);
    }

    /// Records whether the terminal's family lacks a face at its configured
    /// weight. Runs after [`Self::resolve_fonts_with`], so it checks the
    /// family that will draw, and skips one already reported missing.
    pub(super) fn check_font_weight(&mut self, has_weight: impl Fn(&str, FontWeight) -> bool) {
        let terminal = &self.terminal;
        self.missing_weight = (terminal.weight != FontWeight::NORMAL
            && !terminal.family.starts_with('.')
            && !self.missing_fonts.contains(&terminal.family)
            && !has_weight(&terminal.family, terminal.weight))
        .then(|| MissingWeight {
            family: terminal.family.clone(),
            weight: terminal.weight,
        });
    }

    pub(super) fn resolve_fonts_with<I>(
        &mut self,
        installed: impl FnOnce() -> I,
        resolves_alias: impl Fn(&str) -> bool,
    ) where
        I: IntoIterator<Item = String>,
    {
        self.replace_undrawable_fonts(super::bitmap_fonts::is_undrawable);
        let terminal_detects = self.terminal.fallbacks.is_none();
        let configured = [&self.sidebar, &self.tabs, &self.terminal, &self.ui];
        let requested: BTreeSet<&str> = configured
            .into_iter()
            .flat_map(FontConfig::requested)
            .collect();
        let check_defaults = configured
            .iter()
            .any(|face| PLATFORM_FONTS.is_replaceable(&face.family));
        let detect = configured.iter().any(|face| face.fallbacks.is_none());
        if requested.is_empty() && !check_defaults && !detect {
            return;
        }
        let installed: BTreeSet<String> = installed().into_iter().collect();
        self.missing_fonts = missing_families(requested, &installed, resolves_alias);
        let mut faces = [
            &mut self.sidebar,
            &mut self.tabs,
            &mut self.terminal,
            &mut self.ui,
        ];
        if check_defaults {
            for face in faces.iter_mut() {
                if let Some(family) = PLATFORM_FONTS.substitute(&face.family, &installed) {
                    face.family = family;
                }
            }
        }
        if faces.iter().all(|face| face.fallbacks.is_some()) {
            return;
        }
        // A terminal face patched with the icons needs no cascade to draw
        // them, unless it is one of the configured families this machine lacks.
        let [_, _, terminal, _] = &faces;
        let terminal_draws_icons =
            draws_icons(&terminal.family) && !self.missing_fonts.contains(&terminal.family);
        let detected = symbol_fallbacks(installed);
        // Only the terminal draws prompts; an explicit `fallback`, even `[]`,
        // is a choice the user already made.
        self.icon_font_missing = detected.is_empty() && terminal_detects && !terminal_draws_icons;
        for face in faces {
            if face.fallbacks.is_none() {
                face.fallbacks = Some(detected.clone());
            }
        }
    }

    /// Puts each face whose family `undrawable` rejects back on its default,
    /// which [`Self::resolve_fonts`] then substitutes if it is not installed.
    /// The file keeps the chosen family, so a fixed renderer picks it up again.
    pub(super) fn replace_undrawable_fonts(&mut self, undrawable: impl Fn(&str) -> bool) {
        let defaults = Config::default();
        for (face, default) in [
            (&mut self.sidebar, defaults.sidebar),
            (&mut self.tabs, defaults.tabs),
            (&mut self.terminal, defaults.terminal),
            (&mut self.ui, defaults.ui),
        ] {
            if face.family != default.family && undrawable(&face.family) {
                tracing::warn!(
                    family = %face.family,
                    replacement = %default.family,
                    "Font embeds bitmap glyphs the renderer cannot draw; using the default family"
                );
                face.family = default.family;
            }
        }
    }
}
