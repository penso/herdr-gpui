//! px0 shares the Code settings page. Explicit Apply keeps unfinished edits
//! local; saving goes through the settings window's serial persistence path.
// i18n: migrated
use super::*;
use crate::{browser::WebUrl, i18n::fl};

#[derive(Default)]
pub(in crate::settings_window) struct Px0Settings {
    input: Option<Entity<SearchInput>>,
    shown: String,
    invalid: bool,
    /// Exact submitted text, including whitespace, until its save completes.
    saving: Option<String>,
}

#[cfg(test)]
mod tests;

impl SettingsWindow {
    pub(in crate::settings_window) fn open_px0_settings(&mut self, cx: &mut Context<Self>) {
        if self.px0.input.is_none() {
            self.px0.input = Some(cx.new(SearchInput::new));
        }
        self.sync_px0_field(cx);
    }

    pub(in crate::settings_window) fn sync_px0_field(&mut self, cx: &mut Context<Self>) {
        let Some(input) = &self.px0.input else {
            return;
        };
        let saved = self.config.px0.url.as_ref().map_or("", WebUrl::as_str);
        if !self.px0.invalid
            && !input.read(cx).is_composing()
            && input.read(cx).text() == self.px0.shown
        {
            input.update(cx, |input, cx| input.set_text_selected(saved, cx));
            self.px0.shown = saved.to_owned();
        }
        input.update(cx, |input, cx| {
            input.set_placeholder_with(
                || {
                    fl!(
                        "settings-code-px0-placeholder",
                        example = "http://127.0.0.1:7777/"
                    )
                },
                cx,
            );
            input.set_appearance(self.config.ui.clone(), self.theme.clone(), cx);
        });
    }

    fn apply_px0_address(&mut self, cx: &mut Context<Self>) {
        if self.busy() {
            return;
        }
        let Some(input) = &self.px0.input else {
            return;
        };
        if input.read(cx).is_composing() {
            return;
        }
        let text = input.read(cx).text().trim().to_owned();
        let config = if text.is_empty() {
            Some(crate::px0::Config::default())
        } else {
            WebUrl::from_typed(&text)
                .ok()
                .map(|url| crate::px0::Config { url: Some(url) })
        };
        let Some(config) = config.filter(|config| config.check().is_ok()) else {
            self.px0.invalid = true;
            cx.notify();
            return;
        };
        self.px0.invalid = false;
        self.px0.saving = Some(input.read(cx).text().to_owned());
        // Keep a failed write as an edit, rather than replacing it on reload.
        self.save_native(
            move || match config.url {
                Some(url) => Config::set_key("px0.url", url.as_str().into()),
                None => Config::unset_key("px0.url").map(drop),
            },
            cx,
        );
    }

    /// A successful write acknowledges only the submitted text. A newer edit
    /// or composition stays a draft; failures never advance the baseline.
    pub(in crate::settings_window) fn finish_px0_save(&mut self, saved: bool, cx: &App) {
        let Some(submitted) = self.px0.saving.take() else {
            return;
        };
        let Some(input) = &self.px0.input else {
            return;
        };
        if saved && !input.read(cx).is_composing() && input.read(cx).text() == submitted {
            self.px0.shown = submitted;
        }
    }

    pub(in crate::settings_window) fn render_px0_controls(&self, cx: &mut Context<Self>) -> Div {
        self.control_card(&fl!("settings-code-px0"))
            .child(self.control_note(fl!("settings-code-px0-note")))
            .children(self.px0.input.as_ref().map(|input| {
                div()
                    .debug_selector(|| "settings-px0-url".into())
                    .child(input.clone())
            }))
            .when(self.px0.invalid, |card| {
                card.child(self.control_note(fl!("settings-code-px0-invalid")))
            })
            .child(
                self.control_choice(
                    "settings-px0-apply",
                    fl!("settings-code-px0-apply"),
                    false,
                    !self.busy(),
                )
                .when(!self.busy(), |button| {
                    button.on_click(cx.listener(|this, _, _, cx| this.apply_px0_address(cx)))
                }),
            )
    }
}
