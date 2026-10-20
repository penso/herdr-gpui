//! General: interface toggles, the optional `[features]` flags, clipboard
//! feedback, merged worktrees, the browser skill, launch at login (Windows)
//! and the update channel.
use super::{CLIPBOARD_POSITIONS, SettingsWindow, on_off, on_value, ui};
use crate::settings_window::inputs::{DropdownField, NumberField};
use crate::{
    agent_skill::{AgentSkill, Choice},
    config::{ClipboardToast, Feature, UpdateChannel, preferences::Preference},
    herdr_settings::Edit,
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
        let position_names = CLIPBOARD_POSITIONS.map(|(_, name)| name);
        let base_position = CLIPBOARD_POSITIONS
            .iter()
            .find_map(|(p, name)| (*p == base.1).then_some(*name))
            .unwrap_or_default();
        let merged = self.config.remove_merged_worktrees;
        let beta = self.config.updates.channel == UpdateChannel::Beta;
        let installed = AgentSkill::choice(cx) == Some(Choice::Installed);
        let (skill_id, skill_label, choice) = if installed {
            ("general-browser-skill-remove", "Remove", Choice::Declined)
        } else {
            (
                "general-browser-skill-install",
                "Install",
                Choice::Installed,
            )
        };
        div()
            .flex()
            .flex_col()
            .child(ui::section(theme, "Interface", None))
            .child(
                ui::row(theme, "general-ports", "Show listening ports")
                    .description("Port chips on workspaces with a dev server.")
                    .switch(self.preference_toggle(
                        "general-ports-toggle",
                        self.config.show_listening_ports,
                        Preference::ShowListeningPorts(!self.config.show_listening_ports),
                        cx,
                    )),
            )
            .child(
                ui::row(theme, "general-checkpoints", "Checkpoint agent turns")
                    .description("Snapshot the worktree after each agent turn.")
                    .switch(self.preference_toggle(
                        "general-checkpoints-toggle",
                        self.config.agent_checkpoints,
                        Preference::AgentCheckpoints(!self.config.agent_checkpoints),
                        cx,
                    )),
            )
            .child(
                ui::row(theme, "general-smooth-scroll", "Smooth scrolling")
                    .description("Scroll terminal history between rows; off steps whole rows.")
                    .switch(self.preference_toggle(
                        "general-smooth-scroll-toggle",
                        self.config.smooth_scroll,
                        Preference::SmoothScroll(!self.config.smooth_scroll),
                        cx,
                    )),
            )
            .when(cfg!(target_os = "linux"), |page| {
                page.child(
                    ui::row(theme, "general-window-controls", "Show window controls")
                        .description(
                            "Draw minimize, maximize, and close buttons in the menu bar. Disable \
                             server-side decorations in your window manager to avoid a double bar.",
                        )
                        .switch(self.preference_toggle(
                            "general-window-controls-toggle",
                            self.config.show_window_controls,
                            Preference::ShowWindowControls(!self.config.show_window_controls),
                            cx,
                        )),
                )
            })
            .child(ui::section(theme, "Features", None))
            .children(self.feature_rows(cx))
            .child(ui::section(theme, "Clipboard", None))
            .child(
                self.gate_shared(
                    ui::row(theme, "general-copy-on-select", "Copy on select")
                        .tag(ui::Source::Herdr)
                        .description("Off: \u{2318}C / Ctrl-C copies the highlighted selection."),
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
                ui::row(theme, "general-copied-toast", "Copied notification")
                    .tag(ui::Source::HerdrUi)
                    .control(self.override_dropdown(
                        DropdownField::ClipboardEnabled,
                        on_off(base.0),
                        &["On", "Off"],
                        overrides.enabled().map(|on| usize::from(!on)),
                        cx,
                    )),
            )
            .child(
                ui::row(theme, "general-clipboard-position", "Notification position")
                    .tag(ui::Source::HerdrUi)
                    .control(self.override_dropdown(
                        DropdownField::ClipboardPosition,
                        base_position,
                        &position_names,
                        position,
                        cx,
                    )),
            )
            .child(ui::section(theme, "Worktrees", None))
            .child(
                ui::row(
                    theme,
                    "general-merged-worktrees",
                    "Remove worktrees of merged PRs",
                )
                .description(
                    "Deletes a clean linked worktree and closes its workspace once its PR is \
                     merged. The focused workspace asks first.",
                )
                .switch(self.preference_toggle(
                    "general-merged-worktrees-toggle",
                    merged,
                    Preference::RemoveMergedWorktrees(!merged),
                    cx,
                )),
            )
            .child(ui::section(theme, "Browser skill", None))
            .child(
                ui::row(
                    theme,
                    "general-browser-skill",
                    "Teach local agents to use browser tabs",
                )
                .description(if installed {
                    "Installed in ~/.claude and ~/.agents, kept up to date."
                } else {
                    "Not installed."
                })
                .control(
                    ui::button(theme, skill_id, skill_label, true)
                        .on_click(cx.listener(move |this, _, _, cx| this.save_skill(choice, cx))),
                ),
            )
            .children(self.startup_rows(cx))
            .child(ui::section(theme, "Updates", None))
            .child(
                ui::row(theme, "general-update-beta", "Install beta releases")
                    .description(
                        "Betas ship before they are promoted to stable. Turning this off never \
                         downgrades; Homebrew installations update in place while it is on.",
                    )
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
            .child(ui::section(theme, "Links", None))
            .child(self.folder_size_row(
                cx,
                "general-folder-warning",
                "Warn above",
                "Before copying a folder printed in an SSH pane, when its contents are \
                 larger than this.",
                NumberField::FolderWarningMb,
                self.config.remote_files.folder_warning_mb as f32,
            ))
            .child(self.folder_size_row(
                cx,
                "general-folder-cap",
                "Refuse above",
                "A folder archive larger than this is not copied.",
                NumberField::FolderTransferCapMb,
                self.config.remote_files.folder_transfer_cap_mb as f32,
            ))
    }

    /// One `[remote_files]` size row: a typed number in megabytes, no slider.
    fn folder_size_row(
        &self,
        cx: &mut Context<Self>,
        id: &'static str,
        title: &'static str,
        description: &'static str,
        field: NumberField,
        value: f32,
    ) -> impl IntoElement {
        let on_change = on_value(cx, move |this, change, _, cx| {
            this.commit_number(field, change, cx)
        });
        let view = cx.entity().downgrade();
        ui::row(&self.theme, id, title)
            .tag(ui::Source::HerdrUi)
            .description(description)
            .control(ui::number(
                &self.theme,
                ui::Number {
                    id: field.selector(),
                    value,
                    range: field.range(),
                    unit: "MB",
                    slider: false,
                    enabled: true,
                },
                self.number_edit_for(field),
                on_change,
                move |window, cx| {
                    let _ = view.update(cx, |this, cx| {
                        this.begin_number_edit(field, value, window, cx)
                    });
                },
            ))
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
                ui::row(&self.theme, id, feature.label())
                    .description(feature.description())
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
}
