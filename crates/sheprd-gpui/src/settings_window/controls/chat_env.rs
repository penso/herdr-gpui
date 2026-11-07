//! The chats' extra environment on the General page, shown while ACP chats
//! are on. Like px0's address, an edit stays in the field until Apply, and
//! saving goes through the window's serial persistence path.
// i18n: migrated
use super::*;
use crate::{
    config::chat::{env_text, parse_env},
    i18n::fl,
};

#[derive(Default)]
pub(in crate::settings_window) struct ChatEnvSettings {
    input: Option<Entity<SearchInput>>,
    shown: String,
    invalid: bool,
    /// Exact submitted text until its save completes.
    saving: Option<String>,
}

#[cfg(test)]
mod tests;

impl SettingsWindow {
    pub(in crate::settings_window) fn open_chat_env_settings(&mut self, cx: &mut Context<Self>) {
        if self.chat_env.input.is_none() {
            self.chat_env.input = Some(cx.new(SearchInput::new));
        }
        self.sync_chat_env_field(cx);
    }

    pub(in crate::settings_window) fn sync_chat_env_field(&mut self, cx: &mut Context<Self>) {
        let Some(input) = &self.chat_env.input else {
            return;
        };
        let saved = env_text(&self.config.chat.env);
        if !self.chat_env.invalid
            && !input.read(cx).is_composing()
            && input.read(cx).text() == self.chat_env.shown
        {
            input.update(cx, |input, cx| input.set_text_selected(&saved, cx));
            self.chat_env.shown = saved;
        }
        input.update(cx, |input, cx| {
            // i18n: data
            input.set_placeholder("GH_HOST=mycompany.ghe.com ANTHROPIC_BASE_URL=https://…", cx);
            input.set_appearance(self.config.ui.clone(), self.theme.clone(), cx);
        });
    }

    fn apply_chat_env(&mut self, cx: &mut Context<Self>) {
        if self.busy() {
            return;
        }
        let Some(input) = &self.chat_env.input else {
            return;
        };
        if input.read(cx).is_composing() {
            return;
        }
        let Some(env) = parse_env(input.read(cx).text()) else {
            self.chat_env.invalid = true;
            cx.notify();
            return;
        };
        self.chat_env.invalid = false;
        self.chat_env.saving = Some(input.read(cx).text().to_owned());
        self.save_native(
            move || {
                if env.is_empty() {
                    return Config::unset_key("chat.env").map(drop);
                }
                let mut table = toml_edit::InlineTable::new();
                for (name, value) in env {
                    table.insert(&name, value.into());
                }
                Config::set_key("chat.env", toml_edit::Value::InlineTable(table))
            },
            cx,
        );
    }

    /// A successful write acknowledges only the submitted text; a newer
    /// edit stays a draft, and a failure never moves the baseline.
    pub(in crate::settings_window) fn finish_chat_env_save(&mut self, saved: bool, cx: &App) {
        let Some(submitted) = self.chat_env.saving.take() else {
            return;
        };
        let Some(input) = &self.chat_env.input else {
            return;
        };
        if saved && !input.read(cx).is_composing() && input.read(cx).text() == submitted {
            self.chat_env.shown = submitted;
        }
    }

    pub(in crate::settings_window) fn render_chat_env_controls(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<Div> {
        if !self.config.features.acp_chats {
            return None;
        }
        Some(
            self.control_card(&fl!("settings-integrations-chat-env"))
                .child(self.control_note(fl!("settings-integrations-chat-env-note")))
                .children(self.chat_env.input.as_ref().map(|input| {
                    div()
                        .debug_selector(|| "settings-chat-env".into())
                        .child(input.clone())
                }))
                .when(self.chat_env.invalid, |card| {
                    card.child(self.control_note(fl!("settings-integrations-chat-env-invalid")))
                })
                .child(
                    self.control_choice(
                        "settings-chat-env-apply",
                        fl!("settings-integrations-chat-env-apply"),
                        false,
                        !self.busy(),
                    )
                    .when(!self.busy(), |button| {
                        button.on_click(cx.listener(|this, _, _, cx| this.apply_chat_env(cx)))
                    }),
                ),
        )
    }
}
