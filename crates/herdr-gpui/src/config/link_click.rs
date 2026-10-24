//! `link_click_modifier`: the modifiers held to show and open a terminal
//! link.
use std::fmt;

use gpui::Modifiers;
use serde::Deserialize;
use serde::de::{self, Deserializer, SeqAccess, Visitor};

/// What makes a click on a terminal link open it. Alt is never part of it:
/// holding Alt as well opens a link in the target `open_links_in` and
/// `open_files_in` do not name, so it is ignored when matching.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LinkClickModifier {
    /// Unset: Cmd on macOS, Ctrl elsewhere, whatever else is held, as before
    /// the setting existed, so Cmd-Shift-click still opens a link.
    #[default]
    Secondary,
    /// The exact combinations of Cmd, Ctrl, and Shift the user set, any of
    /// which opens a link, none among them for a plain click: one bit per
    /// combination, at the index [`combination`] gives it.
    AnyOf(u8),
}

/// Why a `link_click_modifier` value was refused.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum LinkClickModifierError {
    #[error(
        "link_click_modifier cannot include alt: holding Alt already opens a link in the other \
         target of open_links_in and open_files_in"
    )]
    Alt,
    #[error("link_click_modifier is empty; use \"none\" for a plain click")]
    Empty,
    #[error(
        "unknown link_click_modifier key `{0}`; use cmd, ctrl, or shift joined by `+`, or \"none\""
    )]
    Unknown(String),
}

/// The bit of `modifiers`' Cmd, Ctrl, and Shift combination, whatever else
/// is held.
fn combination(modifiers: Modifiers) -> u8 {
    1 << (u8::from(modifiers.platform)
        | u8::from(modifiers.control) << 1
        | u8::from(modifiers.shift) << 2)
}

impl LinkClickModifier {
    /// Herdr's modifier grammar, as `right_click_passthrough_modifier` reads
    /// it, plus `shift`: `+`-separated `cmd`/`command`/`super` (the platform
    /// key, Super on Linux), `ctrl`/`control`, and `shift`, case-insensitive,
    /// or `none` alone.
    pub(crate) fn parse(value: &str) -> Result<Self, LinkClickModifierError> {
        Self::parse_any([value])
    }

    /// Several combinations in the grammar of [`Self::parse`], any of which
    /// opens a link. An empty list is refused, as an empty value is.
    pub(crate) fn parse_any<'a>(
        values: impl IntoIterator<Item = &'a str>,
    ) -> Result<Self, LinkClickModifierError> {
        let mut allowed = 0;
        for value in values {
            allowed |= combination(Self::parse_one(value)?);
        }
        if allowed == 0 {
            return Err(LinkClickModifierError::Empty);
        }
        Ok(Self::AnyOf(allowed))
    }

    fn parse_one(value: &str) -> Result<Modifiers, LinkClickModifierError> {
        let value = value.trim();
        if value.is_empty() {
            return Err(LinkClickModifierError::Empty);
        }
        if value.eq_ignore_ascii_case("none") {
            return Ok(Modifiers::none());
        }
        let mut modifiers = Modifiers::none();
        for token in value.split('+') {
            match token.trim().to_ascii_lowercase().as_str() {
                "cmd" | "command" | "super" => modifiers.platform = true,
                "ctrl" | "control" => modifiers.control = true,
                "shift" => modifiers.shift = true,
                "alt" | "option" => return Err(LinkClickModifierError::Alt),
                _ => return Err(LinkClickModifierError::Unknown(token.trim().into())),
            }
        }
        Ok(modifiers)
    }

    /// Whether `held` makes a click open a link: the platform key among any
    /// others when unset, or else exactly one of the configured
    /// combinations, apart from Alt, which chooses where the link opens, and
    /// Fn, which no click reads.
    pub(crate) fn matches(self, held: Modifiers) -> bool {
        match self {
            Self::Secondary => held.secondary(),
            Self::AnyOf(allowed) => allowed & combination(held) != 0,
        }
    }
}

impl<'de> Deserialize<'de> for LinkClickModifier {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct OneOrMany;

        impl<'de> Visitor<'de> for OneOrMany {
            type Value = LinkClickModifier;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("a modifier such as \"cmd\", or a list of them")
            }

            fn visit_str<E: de::Error>(self, value: &str) -> Result<Self::Value, E> {
                LinkClickModifier::parse(value).map_err(E::custom)
            }

            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
                let mut values = Vec::new();
                while let Some(value) = seq.next_element::<String>()? {
                    values.push(value);
                }
                LinkClickModifier::parse_any(values.iter().map(String::as_str))
                    .map_err(de::Error::custom)
            }
        }

        deserializer.deserialize_any(OneOrMany)
    }
}
