//! The text the app shows, from Fluent messages (docs/i18n.md). Each feature
//! keeps its English in `locales/en-US/<feature>.ftl`, which the build script
//! embeds and checks. [`fl!`] formats a message in the current locale and
//! fails to compile when its ID or arguments do not match the English file.
//!
//! The locale is a GPUI global, [`CurrentLocale`], chosen by the `language`
//! setting or a QA pseudo-locale. Its catalog is also kept process-wide, as
//! `motion`'s slow switch is, so text is found where there is no app
//! context. Changing it reinstalls the menus and redraws every window; text
//! a view already built keeps its language until it is built again.

mod check;
mod current;
#[cfg(test)]
pub(crate) mod leak_tests;
#[cfg(test)]
mod tests;

pub(crate) use check::check;
pub(crate) use herdr_i18n::FluentValue;
use herdr_i18n::{Catalog, Direction, FluentArgs, Language, Locale, Pseudo};
use std::sync::LazyLock;

include!(concat!(env!("OUT_DIR"), "/messages.rs"));

/// A message in the current locale. `fl!("id")`, or with named arguments
/// `fl!("id", name = value)`; a value is anything Fluent takes, such as a
/// string or a number.
macro_rules! fl {
    ($id:literal $(,)?) => {{
        const _: () = $crate::i18n::check($id, &[]);
        $crate::i18n::text($id, &[])
    }};
    ($id:literal, $($name:ident = $value:expr),+ $(,)?) => {{
        const _: () = $crate::i18n::check($id, &[$(stringify!($name)),+]);
        $crate::i18n::text(
            $id,
            &[$((stringify!($name), $crate::i18n::FluentValue::from($value))),+],
        )
    }};
}
pub(crate) use fl;

/// English without isolation marks, for text that leaves the window.
static ENGLISH: LazyLock<Option<Catalog>> = LazyLock::new(|| Catalog::english(FILES).ok());

/// `id` in the current locale. Only `fl!` names a message by literal, so
/// its ID is checked; [`message`] takes one chosen at run time.
pub(crate) fn text(id: &'static str, args: &[(&str, FluentValue<'_>)]) -> String {
    message(id, args)
}

/// A message whose ID is data, such as a catalog command's. Tests check
/// each such table against the English files.
pub(crate) fn message(id: &str, args: &[(&str, FluentValue<'_>)]) -> String {
    match current::catalog() {
        Some(catalog) => format(&catalog, id, args),
        None => id.to_owned(),
    }
}

/// A message in English, for command line output and searches that match
/// the English name of a translated label.
pub(crate) fn english(id: &str, args: &[(&str, FluentValue<'_>)]) -> String {
    match ENGLISH.as_ref() {
        Some(catalog) => format(catalog, id, args),
        None => id.to_owned(),
    }
}

fn format(catalog: &Catalog, id: &str, args: &[(&str, FluentValue<'_>)]) -> String {
    if args.is_empty() {
        return catalog.text(id);
    }
    let mut fluent = FluentArgs::with_capacity(args.len());
    for (name, value) in args {
        fluent.set(*name, value.clone());
    }
    catalog.text_with(id, &fluent)
}

/// The variables English message `id` takes, or `None` when there is no
/// such message, for tests of tables that name messages as data.
#[cfg(test)]
pub(crate) fn takes(id: &str) -> Option<&'static [&'static str]> {
    MESSAGES
        .iter()
        .find(|(message, _)| *message == id)
        .map(|(_, variables)| *variables)
}

/// A catalog command's name in the current locale.
pub(crate) fn command(info: &crate::controls::CommandInfo) -> String {
    message(info.message, &command_args(info.command))
}

/// A catalog command's English name.
pub(crate) fn command_english(info: &crate::controls::CommandInfo) -> String {
    english(info.message, &command_args(info.command))
}

fn command_args(command: crate::Command) -> Vec<(&'static str, FluentValue<'static>)> {
    command
        .number()
        .map(|number| ("number", FluentValue::from(number)))
        .into_iter()
        .collect()
}

/// The languages with messages; pseudo-locales are not among them.
pub(crate) fn available() -> Vec<Locale> {
    Catalog::available(FILES)
}

/// The locale every window shows, and what chose it.
#[derive(Clone, Debug)]
pub(crate) struct CurrentLocale {
    configured: Language,
    /// QA > Pseudo-Locale, which wins over the setting until turned off.
    pseudo: Option<Pseudo>,
    locale: Locale,
}

impl Default for CurrentLocale {
    fn default() -> Self {
        Self {
            configured: Language::default(),
            pseudo: None,
            locale: Locale::FALLBACK,
        }
    }
}

impl gpui::Global for CurrentLocale {}

impl CurrentLocale {
    pub(crate) fn configured(&self) -> &Language {
        &self.configured
    }

    pub(crate) fn pseudo(&self) -> Option<Pseudo> {
        self.pseudo
    }

    pub(crate) fn locale(&self) -> &Locale {
        &self.locale
    }

    pub(crate) fn direction(&self) -> Direction {
        self.locale.direction()
    }

    fn resolve(configured: Language, pseudo: Option<Pseudo>) -> Self {
        let requested = pseudo.map_or_else(
            || configured.clone(),
            |pseudo| Language::Locale(pseudo.locale()),
        );
        Self {
            locale: requested.resolve(&available(), &herdr_i18n::system_languages()),
            configured,
            pseudo,
        }
    }
}

/// The pseudo-locale the windows show, if they show one, for the QA menu
/// to check. Like Slow Animations, it is read where there is no app context.
pub(crate) fn shown_pseudo() -> Option<Pseudo> {
    current::catalog()?.locale().pseudo()
}

/// Picks the locale the setting asks for when the app starts, before any
/// window or menu is built.
pub(crate) fn install(language: &Language, cx: &mut gpui::App) {
    let state = CurrentLocale::resolve(language.clone(), None);
    current::replace(&state.locale);
    cx.set_global(state);
}

/// Follows a reloaded `language` setting. Each window applies the config it
/// reloads, so only the first that sees a change switches the locale.
pub(crate) fn follow_config(language: &Language, cx: &mut gpui::App) {
    let current = cx
        .try_global::<CurrentLocale>()
        .cloned()
        .unwrap_or_default();
    if current.configured == *language {
        return;
    }
    switch(CurrentLocale::resolve(language.clone(), current.pseudo), cx);
}

/// QA > Pseudo-Locale: shows `pseudo`, or the setting's locale again when
/// `pseudo` is the one shown.
pub(crate) fn toggle_pseudo(pseudo: Pseudo, cx: &mut gpui::App) {
    let current = cx
        .try_global::<CurrentLocale>()
        .cloned()
        .unwrap_or_default();
    let pseudo = (shown_pseudo() != Some(pseudo)).then_some(pseudo);
    switch(CurrentLocale::resolve(current.configured, pseudo), cx);
}

fn switch(state: CurrentLocale, cx: &mut gpui::App) {
    let changed = cx
        .try_global::<CurrentLocale>()
        .is_none_or(|current| current.locale != state.locale);
    let menus = cx
        .try_global::<CurrentLocale>()
        .is_none_or(|current| current.pseudo != state.pseudo);
    if changed {
        current::replace(&state.locale);
    }
    cx.set_global(state);
    if changed || menus {
        // Menus are built in the locale, and the QA menu checks the one shown.
        crate::menus::install(cx);
        cx.refresh_windows();
    }
}
