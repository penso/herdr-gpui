pub type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    // keep-sorted start
    #[error("{tag:?} is not a BCP 47 language tag")]
    InvalidLocale {
        tag: String,
        #[source]
        source: unic_langid::LanguageIdentifierError,
    },
    #[error("could not load the {locale} messages")]
    Load {
        locale: String,
        #[source]
        source: i18n_embed::I18nEmbedError,
    },
    // keep-sorted end
}
