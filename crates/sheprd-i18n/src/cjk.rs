use unic_langid::LanguageIdentifier;

/// Which regional glyph shapes a language wants for Han characters. Unicode
/// gives one codepoint to a character China, Taiwan, Hong Kong, Japan, and
/// Korea each draw a little differently, so the font that draws it must be
/// chosen by language, not by codepoint.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CjkScript {
    /// Mainland China and Singapore.
    Simplified,
    /// Taiwan.
    Traditional,
    /// Hong Kong and Macau, and Cantonese.
    HongKong,
    Japanese,
    Korean,
}

impl CjkScript {
    /// The script a language is written in, or `None` when it is not one of
    /// the Chinese, Japanese, or Korean languages.
    pub fn of(language: &LanguageIdentifier) -> Option<Self> {
        let script = language.script.as_ref().map(|script| script.as_str());
        let region = language.region.as_ref().map(|region| region.as_str());
        match language.language.as_str() {
            "ja" => Some(Self::Japanese),
            "ko" => Some(Self::Korean),
            "zh" => Some(match (script, region) {
                (Some("Hans"), _) => Self::Simplified,
                (Some("Hant") | None, Some("HK" | "MO")) => Self::HongKong,
                (Some("Hant"), _) | (None, Some("TW")) => Self::Traditional,
                _ => Self::Simplified,
            }),
            "yue" => Some(match script {
                Some("Hans") => Self::Simplified,
                _ => Self::HongKong,
            }),
            _ => None,
        }
    }

    /// The script of the first of `languages`, most preferred first, that is
    /// written in one.
    pub fn preferred<'a>(
        languages: impl IntoIterator<Item = &'a LanguageIdentifier>,
    ) -> Option<Self> {
        languages.into_iter().find_map(Self::of)
    }
}
