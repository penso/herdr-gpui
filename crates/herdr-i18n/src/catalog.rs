use crate::{Direction, Error, Locale, Pseudo, Result, pseudo};
use fluent::FluentArgs;
use i18n_embed::{I18nAssets, LanguageLoader, fluent::FluentLanguageLoader};
use std::borrow::Cow;

/// One embedded `.ftl` file: a feature's messages in one language.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct File {
    /// The BCP 47 tag of the directory it is in, such as `en-US`.
    pub language: &'static str,
    /// The feature it belongs to, the file's name without `.ftl`.
    pub feature: &'static str,
    pub source: &'static str,
}

/// i18n-embed looks for one `{language}/{domain}.ftl` per language; every
/// feature file of that language answers for it.
const DOMAIN: &str = "herdr";

struct Files<'a>(&'a [File]);

impl Files<'_> {
    fn of(&self, language: &str) -> impl Iterator<Item = &File> {
        self.0.iter().filter(move |file| file.language == language)
    }
}

impl I18nAssets for Files<'_> {
    fn get_files(&self, path: &str) -> Vec<Cow<'_, [u8]>> {
        let Some((language, _)) = path.split_once('/') else {
            return Vec::new();
        };
        // A pseudo-locale is generated from the fallback's messages.
        let fallback = Locale::FALLBACK.to_string();
        let language = match language.parse::<Locale>() {
            Ok(locale) if locale.pseudo().is_some() => fallback.as_str(),
            _ => language,
        };
        self.of(language)
            .map(|file| Cow::Borrowed(file.source.as_bytes()))
            .collect()
    }

    fn filenames_iter(&self) -> Box<dyn Iterator<Item = String> + '_> {
        let mut languages: Vec<_> = self.0.iter().map(|file| file.language).collect();
        languages.sort_unstable();
        languages.dedup();
        Box::new(
            languages
                .into_iter()
                .map(|language| format!("{language}/{DOMAIN}.ftl")),
        )
    }
}

/// The messages of one locale, with English under every one it lacks.
#[derive(Debug)]
pub struct Catalog {
    locale: Locale,
    loader: FluentLanguageLoader,
}

impl Catalog {
    /// `locale`'s messages; a locale without files of its own shows the
    /// fallback's, under its own name.
    pub fn new(files: &[File], locale: Locale) -> Result<Self> {
        let files = Files(files);
        let loaded = if locale.pseudo().is_some() || files.of(&locale.to_string()).next().is_some()
        {
            locale.clone()
        } else {
            Locale::FALLBACK
        };
        let loader = FluentLanguageLoader::new(DOMAIN, Locale::FALLBACK.id().clone());
        loader
            .load_languages(&files, &[loaded.id().clone()])
            .map_err(|source| Error::Load {
                locale: loaded.to_string(),
                source,
            })?;
        match locale.pseudo() {
            Some(Pseudo::Accented) => {
                loader.with_bundles_mut(|bundle| bundle.set_transform(Some(pseudo::accented)));
            }
            Some(Pseudo::Bidi) => {
                loader.with_bundles_mut(|bundle| bundle.set_transform(Some(pseudo::bidi)));
            }
            None => {}
        }
        Ok(Self { locale, loader })
    }

    /// The fallback's English without Unicode isolation marks, for text
    /// that leaves the window: command line output, and searches that
    /// match the English name of a translated label.
    pub fn english(files: &[File]) -> Result<Self> {
        let catalog = Self::new(files, Locale::FALLBACK)?;
        catalog.loader.set_use_isolating(false);
        Ok(catalog)
    }

    /// The languages with messages, the fallback first. Pseudo-locales are
    /// not among them: the system never negotiates one.
    pub fn available(files: &[File]) -> Vec<Locale> {
        let mut available = vec![Locale::FALLBACK];
        for locale in Files(files)
            .filenames_iter()
            .filter_map(|path| path.split_once('/')?.0.parse::<Locale>().ok())
        {
            if !available.contains(&locale) {
                available.push(locale);
            }
        }
        available
    }

    pub fn locale(&self) -> &Locale {
        &self.locale
    }

    pub fn direction(&self) -> Direction {
        self.locale.direction()
    }

    pub fn has(&self, id: &str) -> bool {
        self.loader.has(id)
    }

    pub fn text(&self, id: &str) -> String {
        self.finish(self.loader.get(id))
    }

    pub fn text_with(&self, id: &str, args: &FluentArgs<'_>) -> String {
        self.finish(self.loader.get_args_fluent(id, Some(args)))
    }

    fn finish(&self, text: String) -> String {
        match self.locale.pseudo() {
            Some(Pseudo::Accented) => pseudo::mark(text),
            Some(Pseudo::Bidi) | None => text,
        }
    }
}
