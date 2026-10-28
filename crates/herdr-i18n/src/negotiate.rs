use crate::{Language, Locale};
use fluent_langneg::{NegotiationStrategy, negotiate_languages};
use unic_langid::LanguageIdentifier;

/// The languages the OS prefers, most preferred first. Tags it reports in
/// POSIX form (`pt_BR.UTF-8@euro`) are read as BCP 47; ones that still do
/// not parse are skipped.
pub fn system_languages() -> Vec<LanguageIdentifier> {
    sys_locale::get_locales()
        .filter_map(|tag| posix_to_bcp47(&tag).parse().ok())
        .collect()
}

pub(crate) fn posix_to_bcp47(tag: &str) -> String {
    let tag = tag.split(['.', '@']).next().unwrap_or(tag);
    tag.replace('_', "-")
}

/// The best of `available` for `requested`, in the order the user prefers
/// them, else [`Locale::FALLBACK`].
pub fn negotiate<R: AsRef<LanguageIdentifier>>(requested: &[R], available: &[Locale]) -> Locale {
    let available: Vec<&LanguageIdentifier> = available.iter().map(Locale::id).collect();
    negotiate_languages(
        requested,
        &available,
        Some(&Locale::FALLBACK.id()),
        NegotiationStrategy::Filtering,
    )
    .first()
    .map_or(Locale::FALLBACK, |id| Locale::from((**id).clone()))
}

impl Language {
    /// The locale to show for this setting, given the languages that have
    /// messages and the OS's preferences. A pseudo-locale named here is
    /// shown as is; the system never asks for one.
    pub fn resolve(&self, available: &[Locale], system: &[LanguageIdentifier]) -> Locale {
        match self {
            Self::Locale(locale) if locale.pseudo().is_some() => locale.clone(),
            Self::Locale(locale) => negotiate(&[locale], available),
            Self::System => negotiate(system, available),
        }
    }
}
