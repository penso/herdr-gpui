//! The terminal's configured font weight, the heavier weight its bold cells
//! use, and whether the configured family has a face at that weight.
use super::Error;
use crate::Result;
use gpui::FontWeight;

/// CSS weight names, lightest first. The first name for each weight is the
/// one diagnostics print.
const NAMED_WEIGHTS: [(&str, FontWeight); 14] = [
    ("thin", FontWeight::THIN),
    ("extralight", FontWeight::EXTRA_LIGHT),
    ("ultralight", FontWeight::EXTRA_LIGHT),
    ("light", FontWeight::LIGHT),
    ("normal", FontWeight::NORMAL),
    ("regular", FontWeight::NORMAL),
    ("medium", FontWeight::MEDIUM),
    ("semibold", FontWeight::SEMIBOLD),
    ("demibold", FontWeight::SEMIBOLD),
    ("bold", FontWeight::BOLD),
    ("extrabold", FontWeight::EXTRA_BOLD),
    ("ultrabold", FontWeight::EXTRA_BOLD),
    ("black", FontWeight::BLACK),
    ("heavy", FontWeight::BLACK),
];

/// How far bold cells sit above the configured weight: regular text's 400
/// becomes the conventional 700.
const BOLD_STEP: f32 = 300.0;

/// Reads a `weight` value: a CSS name such as `"medium"`, ignoring case,
/// spaces, hyphens and underscores, or a number from 100 to 900. Untyped
/// because only the terminal takes the key, so elsewhere its type must not
/// fail the whole file before it is reported as unknown.
pub(super) fn parse(value: toml::Value, name: &'static str) -> Result<FontWeight> {
    let weight = match value {
        toml::Value::String(text) => {
            let normalized: String = text
                .chars()
                .filter(|c| !matches!(c, ' ' | '-' | '_'))
                .flat_map(char::to_lowercase)
                .collect();
            NAMED_WEIGHTS
                .iter()
                .find(|(weight_name, _)| *weight_name == normalized)
                .map(|(_, weight)| *weight)
        }
        toml::Value::Integer(value) => (100..=900)
            .contains(&value)
            .then_some(FontWeight(value as f32)),
        _ => None,
    };
    weight.ok_or_else(|| Error::InvalidFontWeight(name).into())
}

/// The weight bold cells draw at: a fixed step heavier than `weight`, never
/// lighter than bold, so bold stays visible over a medium or light base.
pub fn bold(weight: FontWeight) -> FontWeight {
    FontWeight((weight.0 + BOLD_STEP).clamp(FontWeight::BOLD.0, FontWeight::BLACK.0))
}

/// `weight` as configured: its CSS name with its number, or the number alone.
pub(super) fn describe(weight: FontWeight) -> String {
    match NAMED_WEIGHTS.iter().find(|(_, named)| *named == weight) {
        Some((name, _)) => format!("{name} ({})", weight.0),
        None => format!("{}", weight.0),
    }
}

/// A configured weight the terminal's family has no face for, so the platform
/// draws the nearest weight it has instead.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct MissingWeight {
    pub(crate) family: String,
    pub(crate) weight: FontWeight,
}

/// Whether `family` has a face within half a step of `weight`. Core Text
/// reports weights through a piecewise mapping, so an exact match is rare.
/// Loads every face of the family, so it must stay off the UI thread.
#[cfg(target_os = "macos")]
pub(super) fn platform_has_weight(family: &str, weight: FontWeight) -> bool {
    let Ok(handle) = font_kit::source::SystemSource::new().select_family_by_name(family) else {
        // A missing family is reported on its own.
        return true;
    };
    handle
        .fonts()
        .iter()
        .filter_map(|face| face.load().ok())
        .any(|face| (face.properties().weight.0 - weight.0).abs() < 50.0)
}

/// Only the macOS backend lists a family's faces by weight here, so other
/// platforms report nothing rather than guess.
#[cfg(not(target_os = "macos"))]
pub(super) fn platform_has_weight(_family: &str, _weight: FontWeight) -> bool {
    true
}
