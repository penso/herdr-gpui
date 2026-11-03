//! General > Language: the `language` setting, offered as System and every
//! language with messages, each named in itself so a reader finds their own
//! whatever the windows show.
// i18n: migrated
use super::{SettingsWindow, ui};
use crate::{
    i18n::{self, fl},
    settings_window::inputs::DropdownField,
};
use gpui::{prelude::*, *};
use herdr_i18n::{Language, LanguageIdentifier, Locale};

/// The picker's choices in order: the system's languages, each language
/// with messages, and the configured tag when it has none, so the setting
/// shows as it is rather than as a choice it is not.
pub(in crate::settings_window) fn language_choices(configured: &Language) -> Vec<Language> {
    let mut choices: Vec<_> = std::iter::once(Language::System)
        .chain(i18n::available().into_iter().map(Language::Locale))
        .collect();
    if !choices.contains(configured) {
        choices.push(configured.clone());
    }
    choices
}

/// A choice as the picker names it: a language by its endonym, System and
/// a tag without messages by what they show, and a pseudo-locale by its tag.
fn label(language: &Language, available: &[Locale], system: &[LanguageIdentifier]) -> String {
    let shown = |language: &Language| i18n::endonym(&language.resolve(available, system));
    match language {
        Language::System => fl!(
            "settings-general-language-system",
            language = shown(language)
        ),
        Language::Locale(locale) if available.contains(locale) => i18n::endonym(locale),
        Language::Locale(locale) if locale.pseudo().is_some() => locale.to_string(),
        Language::Locale(locale) => fl!(
            "settings-general-language-unavailable",
            tag = locale.to_string(),
            language = shown(language)
        ),
    }
}

impl SettingsWindow {
    pub(in crate::settings_window) fn language_row(&self, cx: &mut Context<Self>) -> ui::Row {
        const FIELD: DropdownField = DropdownField::Language;
        let configured = &self.config.language;
        let available = i18n::available();
        let system = herdr_i18n::system_languages();
        let choices = language_choices(configured);
        let selected = choices
            .iter()
            .position(|choice| choice == configured)
            .unwrap_or(0);
        let options: Vec<SharedString> = choices
            .iter()
            .map(|choice| SharedString::from(label(choice, &available, &system)))
            .collect();
        let current = options.get(selected).cloned().unwrap_or_default();
        let view = cx.entity().downgrade();
        let (opener, picker, dismisser) = (view.clone(), view.clone(), view);
        ui::row(
            &self.theme,
            "general-language",
            fl!("settings-general-language"),
        )
        .description(fl!("settings-general-language-description"))
        .control(ui::dropdown(
            &self.theme,
            FIELD.selector(),
            current,
            false,
            !self.quitting && self.closing.is_none(),
            self.dropdown.as_ref().filter(|open| open.field == FIELD),
            move |window, cx| {
                let options = options.clone();
                let _ = opener.update(cx, |this, cx| {
                    this.open_dropdown(FIELD, options, selected, window, cx)
                });
            },
            move |index, window, cx| {
                let _ = picker.update(cx, |this, cx| {
                    this.pick_dropdown(FIELD, index, cx);
                    window.focus(&this.focus, cx);
                });
            },
            move |window, cx| {
                let _ = dismisser.update(cx, |this, cx| this.close_dropdown(window, cx));
            },
        ))
    }
}
