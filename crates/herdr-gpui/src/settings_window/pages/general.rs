//! General: interface toggles, the optional `[features]` flags, clipboard
//! feedback, merged worktrees, the browser skill, launch at login (Windows)
//! and the update channel.
// i18n: migrated
use super::{
    CLIPBOARD_POSITIONS, SettingsWindow, on_off, on_off_choices, on_value, position_name,
    position_names, ui,
};
use crate::settings_window::inputs::{DropdownField, NumberField};
use crate::{
    agent_skill::{AgentSkill, Choice},
    config::{
        ChatLauncher, ChatLayoutMode, ClipboardToast, Feature, UpdateChannel,
        preferences::Preference,
    },
    herdr_settings::Edit,
    i18n::{self, fl},
};
use gpui::{prelude::*, *};

impl SettingsWindow {
    pub(in crate::settings_window) fn render_general(&self, cx: &mut Context<Self>) -> Div {
        let theme = &self.theme;
        let ready = self.controls_shared_ready();
        let copy = self
            .shared
            .as_ref()
            .is_none_or(|shared| shared.copy_on_select);
        let base = self.shared.as_ref().map_or_else(
            || {
                let default = ClipboardToast::default();
                (default.enabled, default.position)
            },
            |shared| (shared.clipboard.enabled, shared.clipboard.position),
        );
        let overrides = self.config.clipboard_overrides;
        let position = overrides
            .position()
            .and_then(|p| CLIPBOARD_POSITIONS.iter().position(|(c, _)| *c == p));
        let position_names = position_names(&CLIPBOARD_POSITIONS);
        let base_position = position_name(&CLIPBOARD_POSITIONS, &base.1);
        let merged = self.config.remove_merged_worktrees;
        let beta = self.config.updates.channel == UpdateChannel::Beta;
        let installed = AgentSkill::choice(cx) == Some(Choice::Installed);
        let (skill_id, skill_label, choice) = if installed {
            (
                "general-browser-skill-remove",
                fl!("settings-general-browser-skill-remove"),
                Choice::Declined,
            )
        } else {
            (
                "general-browser-skill-install",
                fl!("settings-general-browser-skill-install"),
                Choice::Installed,
            )
        };
        div()
            .flex()
            .flex_col()
            .child(ui::section(
                theme,
                &fl!("settings-general-section-interface"),
                None,
            ))
            .child(
                ui::row(theme, "general-ports", fl!("settings-general-ports"))
                    .description(fl!("settings-general-ports-description"))
                    .switch(self.preference_toggle(
                        "general-ports-toggle",
                        self.config.show_listening_ports,
                        Preference::ShowListeningPorts(!self.config.show_listening_ports),
                        cx,
                    )),
            )
            .child(
                ui::row(
                    theme,
                    "general-checkpoints",
                    fl!("settings-general-checkpoints"),
                )
                .description(fl!("settings-general-checkpoints-description"))
                .switch(self.preference_toggle(
                    "general-checkpoints-toggle",
                    self.config.agent_checkpoints,
                    Preference::AgentCheckpoints(!self.config.agent_checkpoints),
                    cx,
                )),
            )
            .child(
                ui::row(
                    theme,
                    "general-smooth-scroll",
                    fl!("settings-general-smooth-scroll"),
                )
                .description(fl!("settings-general-smooth-scroll-description"))
                .switch(self.preference_toggle(
                    "general-smooth-scroll-toggle",
                    self.config.smooth_scroll,
                    Preference::SmoothScroll(!self.config.smooth_scroll),
                    cx,
                )),
            )
            .when(cfg!(target_os = "linux"), |page| {
                page.child(
                    ui::row(
                        theme,
                        "general-window-controls",
                        fl!("settings-general-window-controls"),
                    )
                    .description(fl!("settings-general-window-controls-description"))
                    .switch(self.preference_toggle(
                        "general-window-controls-toggle",
                        self.config.show_window_controls,
                        Preference::ShowWindowControls(!self.config.show_window_controls),
                        cx,
                    )),
                )
            })
            .child(ui::section(
                theme,
                &fl!("settings-general-section-features"),
                None,
            ))
            .children(self.feature_rows(cx))
            .children(self.config.features.acp_chats.then(|| {
                let layouts: Vec<_> = ChatLayoutMode::ALL
                    .iter()
                    .map(|&mode| (mode, mode.label()))
                    .collect();
                ui::row(
                    theme,
                    "general-chat-layout",
                    fl!("settings-general-chat-layout"),
                )
                .description(fl!("settings-general-chat-layout-description"))
                .control(ui::segmented(
                    theme,
                    "general-chat-layout-control",
                    &layouts,
                    Some(self.config.chat.layout),
                    !self.quitting && self.closing.is_none(),
                    on_value(cx, |this, mode, _, cx| {
                        this.save_preference(Preference::ChatLayout(mode), cx)
                    }),
                ))
            }))
            .children(
                self.config
                    .features
                    .acp_chats
                    .then(|| self.chat_width_controls(cx)),
            )
            .children(self.config.features.acp_chats.then(|| {
                let launchers: Vec<_> = ChatLauncher::ALL
                    .iter()
                    .map(|&launcher| (launcher, launcher.label()))
                    .collect();
                ui::row(
                    theme,
                    "general-chat-launcher",
                    fl!("settings-general-chat-launcher"),
                )
                .description(fl!("settings-general-chat-launcher-description"))
                .control(ui::segmented(
                    theme,
                    "general-chat-launcher-control",
                    &launchers,
                    Some(self.config.chat.launcher),
                    !self.quitting && self.closing.is_none(),
                    on_value(cx, |this, launcher, _, cx| {
                        this.save_preference(Preference::ChatLauncher(launcher), cx)
                    }),
                ))
            }))
            .children(self.config.features.acp_chats.then(|| {
                ui::row(
                    theme,
                    "general-chat-glide",
                    fl!("settings-general-chat-glide"),
                )
                .description(fl!("settings-general-chat-glide-description"))
                .switch(self.preference_toggle(
                    "general-chat-glide-toggle",
                    self.config.chat.glide,
                    Preference::ChatGlide(!self.config.chat.glide),
                    cx,
                ))
            }))
            .children(self.render_chat_env_controls(cx))
            .child(ui::section(
                theme,
                &fl!("settings-general-section-clipboard"),
                None,
            ))
            .child(
                self.gate_shared(
                    ui::row(
                        theme,
                        "general-copy-on-select",
                        fl!("settings-general-copy-on-select"),
                    )
                    .tag(ui::Source::Herdr)
                    .description(fl!("settings-general-copy-on-select-description")),
                )
                .switch(
                    ui::toggle(theme, "general-copy-on-select-toggle", copy, ready).when(
                        ready,
                        |toggle| {
                            toggle.on_click(cx.listener(move |this, _, _, cx| {
                                this.save_shared(Edit::CopyOnSelect(!copy), cx)
                            }))
                        },
                    ),
                ),
            )
            .child(
                ui::row(
                    theme,
                    "general-copied-toast",
                    fl!("settings-general-copied-toast"),
                )
                .tag(ui::Source::HerdrUi)
                .control(self.override_dropdown(
                    DropdownField::ClipboardEnabled,
                    &on_off(base.0),
                    &on_off_choices(),
                    overrides.enabled().map(|on| usize::from(!on)),
                    cx,
                )),
            )
            .child(
                ui::row(
                    theme,
                    "general-clipboard-position",
                    fl!("settings-general-clipboard-position"),
                )
                .tag(ui::Source::HerdrUi)
                .control(self.override_dropdown(
                    DropdownField::ClipboardPosition,
                    &base_position,
                    &position_names,
                    position,
                    cx,
                )),
            )
            .child(ui::section(
                theme,
                &fl!("settings-general-section-worktrees"),
                None,
            ))
            .child(
                ui::row(
                    theme,
                    "general-merged-worktrees",
                    fl!("settings-general-merged-worktrees"),
                )
                .description(fl!("settings-general-merged-worktrees-description"))
                .switch(self.preference_toggle(
                    "general-merged-worktrees-toggle",
                    merged,
                    Preference::RemoveMergedWorktrees(!merged),
                    cx,
                )),
            )
            .child(ui::section(
                theme,
                &fl!("settings-general-section-browser-skill"),
                None,
            ))
            .child(
                ui::row(
                    theme,
                    "general-browser-skill",
                    fl!("settings-general-browser-skill"),
                )
                .description(if installed {
                    fl!("settings-general-browser-skill-description")
                } else {
                    fl!("settings-general-browser-skill-missing")
                })
                .control(
                    ui::button(theme, skill_id, skill_label, true)
                        .on_click(cx.listener(move |this, _, _, cx| this.save_skill(choice, cx))),
                ),
            )
            .children(self.startup_rows(cx))
            .child(ui::section(
                theme,
                &fl!("settings-general-section-updates"),
                None,
            ))
            .child(
                ui::row(
                    theme,
                    "general-update-beta",
                    fl!("settings-general-update-beta"),
                )
                .description(fl!("settings-general-update-beta-description"))
                .switch(self.preference_toggle(
                    "general-update-beta-toggle",
                    beta,
                    Preference::UpdateChannel(if beta {
                        UpdateChannel::Stable
                    } else {
                        UpdateChannel::Beta
                    }),
                    cx,
                )),
            )
    }

    /// Windows' **Launch at login**; other platforms have no such setting yet.
    fn startup_rows(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        #[cfg(windows)]
        return self.launch_at_login_rows(cx);
        #[cfg(not(windows))]
        {
            let _ = cx;
            Vec::new()
        }
    }

    /// One row per `[features]` flag, generated from [`Feature::ALL`] so a
    /// flag added to the table shows up here on its own.
    fn feature_rows(&self, cx: &mut Context<Self>) -> Vec<ui::Row> {
        let features = self.config.features;
        Feature::ALL
            .iter()
            .map(|&feature| {
                let id = feature.setting_id();
                let on = features.enabled(feature);
                ui::row(&self.theme, id, i18n::message(feature.message(), &[]))
                    .description(i18n::message(feature.description_message(), &[]))
                    .switch(self.preference_toggle(
                        format!("{id}-toggle"),
                        on,
                        Preference::Feature(feature, !on),
                        cx,
                    ))
            })
            .collect()
    }

    pub(in crate::settings_window) fn save_skill(
        &mut self,
        choice: Choice,
        cx: &mut Context<Self>,
    ) {
        self.save_skill_with(
            choice,
            move || {
                let home = crate::config::home()?;
                match choice {
                    Choice::Installed => {
                        let text =
                            crate::agent_skill::text(crate::agent_skill::executable().as_deref());
                        crate::agent_skill::install(&home, &text)?;
                    }
                    Choice::Declined => {
                        crate::agent_skill::remove(&home)?;
                    }
                }
                Ok(())
            },
            self.loader(cx),
            cx,
        );
    }

    pub(in crate::settings_window) fn save_skill_with(
        &mut self,
        choice: Choice,
        operation: impl FnOnce() -> crate::Result<()> + Send + 'static,
        load: impl FnOnce() -> crate::Result<super::super::Loaded> + Send + 'static,
        cx: &mut Context<Self>,
    ) {
        self.save_with_completion(
            operation,
            load,
            false,
            move |cx| AgentSkill::choose(choice, cx),
            cx,
        );
    }

    fn chat_width_controls(&self, cx: &mut Context<Self>) -> Div {
        let full = self.config.chat.max_width == 0;
        let field = NumberField::ChatMaxWidth;
        let value = self.number_value(field, f32::from(self.config.chat.max_width));
        let view = cx.entity().downgrade();
        div()
            .child(
                ui::row(
                    &self.theme,
                    "general-chat-full-width",
                    fl!("settings-general-chat-full-width"),
                )
                .description(fl!("settings-general-chat-full-width-description"))
                .switch(self.preference_toggle(
                    "general-chat-full-width-toggle",
                    full,
                    Preference::ChatMaxWidth(if full {
                        crate::config::chat::DEFAULT_MAX_WIDTH
                    } else {
                        0
                    }),
                    cx,
                )),
            )
            .when(!full, |controls| {
                controls.child(
                    ui::row(
                        &self.theme,
                        "general-chat-width",
                        fl!("settings-general-chat-width"),
                    )
                    .description(fl!("settings-general-chat-width-description"))
                    .control(ui::number(
                        &self.theme,
                        ui::Number {
                            id: field.selector(),
                            value,
                            range: field.range(),
                            unit: ui::Unit::Pixels,
                            look: ui::Look::Slider,
                            enabled: !self.quitting && self.closing.is_none(),
                        },
                        self.number_edit_for(field),
                        on_value(cx, move |this, change, _, cx| {
                            this.commit_number(field, change, cx)
                        }),
                        move |window, cx| {
                            let _ = view.update(cx, |this, cx| {
                                this.begin_number_edit(field, value, window, cx)
                            });
                        },
                    )),
                )
            })
    }
}
