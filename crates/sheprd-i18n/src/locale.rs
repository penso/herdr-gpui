use crate::{Error, Result};
use std::{fmt, str::FromStr};
use unic_langid::{CharacterDirection, LanguageIdentifier, langid};

/// A language the app can show, by its BCP 47 tag.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Locale(LanguageIdentifier);

impl Locale {
    /// The language every message has, and the one shown when no other
    /// matches.
    pub const FALLBACK: Self = Self(langid!("en-US"));

    pub fn id(&self) -> &LanguageIdentifier {
        &self.0
    }

    /// The pseudo-locale this is, if it is one.
    pub fn pseudo(&self) -> Option<Pseudo> {
        Pseudo::ALL
            .into_iter()
            .find(|pseudo| pseudo.locale() == *self)
    }

    /// Which way the language's text runs. Pseudo-locales are tagged as
    /// English but `en-XB` runs right to left.
    pub fn direction(&self) -> Direction {
        match self.pseudo() {
            Some(Pseudo::Bidi) => Direction::Rtl,
            Some(Pseudo::Accented) => Direction::Ltr,
            None => match self.0.character_direction() {
                CharacterDirection::RTL => Direction::Rtl,
                CharacterDirection::LTR | CharacterDirection::TTB => Direction::Ltr,
            },
        }
    }
}

impl From<LanguageIdentifier> for Locale {
    fn from(id: LanguageIdentifier) -> Self {
        Self(id)
    }
}

impl AsRef<LanguageIdentifier> for Locale {
    fn as_ref(&self) -> &LanguageIdentifier {
        &self.0
    }
}

impl FromStr for Locale {
    type Err = Error;

    fn from_str(tag: &str) -> Result<Self> {
        tag.parse()
            .map(Self)
            .map_err(|source| Error::InvalidLocale {
                tag: tag.to_owned(),
                source,
            })
    }
}

impl fmt::Display for Locale {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// Which way a language's text runs, and so which way the layout mirrors.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Direction {
    #[default]
    Ltr,
    Rtl,
}

impl Direction {
    pub fn name(self) -> &'static str {
        match self {
            Self::Ltr => "ltr",
            Self::Rtl => "rtl",
        }
    }
}

/// Generated English for testing, never written as files: `en-XA` shows
/// every translated message accented, longer, and bracketed, so untranslated
/// text and clipped labels stand out; `en-XB` shows it right to left.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Deserialize)]
pub enum Pseudo {
    Accented,
    Bidi,
}

impl Pseudo {
    pub const ALL: [Self; 2] = [Self::Accented, Self::Bidi];

    pub fn locale(self) -> Locale {
        Locale(match self {
            Self::Accented => langid!("en-XA"),
            Self::Bidi => langid!("en-XB"),
        })
    }
}

/// The `language` setting: the system's languages, or one tag.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Language {
    #[default]
    System,
    Locale(Locale),
}

impl Language {
    const SYSTEM: &str = "system";
}

impl FromStr for Language {
    type Err = Error;

    fn from_str(value: &str) -> Result<Self> {
        if value.eq_ignore_ascii_case(Self::SYSTEM) {
            return Ok(Self::System);
        }
        value.parse().map(Self::Locale)
    }
}

impl fmt::Display for Language {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::System => f.write_str(Self::SYSTEM),
            Self::Locale(locale) => locale.fmt(f),
        }
    }
}

impl serde::Serialize for Language {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de> serde::Deserialize<'de> for Language {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        value.parse().map_err(serde::de::Error::custom)
    }
}
