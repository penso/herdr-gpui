use crate::{Language, Locale};
use fluent_langneg::{LanguageIdentifier as Matched, NegotiationStrategy, negotiate_languages};
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
///
/// `fluent-langneg` matches its own identifier type (`icu_locid`'s), while
/// Fluent and the catalog keep `unic-langid`'s, so both sides are converted
/// through their BCP 47 form here. A tag the matcher cannot read takes no
/// part, as an unparsable system tag does not.
pub fn negotiate<R: AsRef<LanguageIdentifier>>(requested: &[R], available: &[Locale]) -> Locale {
    let requested: Vec<Matched> = requested
        .iter()
        .filter_map(|id| matched(id.as_ref()))
        .collect();
    let (candidates, locales): (Vec<Matched>, Vec<&Locale>) = available
        .iter()
        .filter_map(|locale| Some((matched(locale.id())?, locale)))
        .unzip();
    negotiate_languages(
        &requested,
        &candidates,
        None,
        NegotiationStrategy::Filtering,
    )
    .first()
    .and_then(|best| candidates.iter().position(|id| id == *best))
    .and_then(|index| locales.get(index))
    .map_or(Locale::FALLBACK, |locale| (*locale).clone())
}

/// `id` as the identifier `fluent-langneg` matches.
fn matched(id: &LanguageIdentifier) -> Option<Matched> {
    id.to_string().parse().ok()
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
