//! Tabs & panes: tab layout and bar, closing confirmations, pane focus and
//! session restore.
use super::{SettingsWindow, on_value, ui};
use crate::{
    config::{TabLayoutMode, preferences::Preference},
    herdr_settings::{Edit, TabBarPosition},
    settings_window::remote_history::HostState,
};
use gpui::{prelude::*, *};

impl SettingsWindow {
    pub(in crate::settings_window) fn render_tabs(&self, cx: &mut Context<Self>) -> Div {
        let theme = &self.theme;
        let ready = self.controls_shared_ready();
        let shared = self.shared.as_ref();
        let position = shared.map(|shared| shared.tab_bar_position);
        let hide = shared.is_some_and(|shared| shared.hide_tab_bar_when_single_tab);
        let history = shared.is_some_and(|shared| shared.pane_history);
        let layouts: Vec<_> = TabLayoutMode::ALL
            .iter()
            .map(|&mode| (mode, mode.label()))
            .collect();
        let open = !self.quitting && self.closing.is_none();
        let mut page = div()
            .flex()
            .flex_col()
            .child(ui::section(theme, "Layout", None))
            .child(
                ui::row(theme, "tabs-layout", "Tab layout")
                    .description("How tab strips draw their tabs. Also under View > Tab Layout.")
                    .control(ui::segmented(
                        theme,
                        "tabs-layout-control",
                        &layouts,
                        Some(self.config.tab_layout),
                        open,
                        on_value(cx, |this, mode, _, cx| {
                            this.save_preference(Preference::TabLayout(mode), cx)
                        }),
                    )),
            )
            .child(ui::section(theme, "Tab bar", Some(ui::Source::Herdr)))
            .child(
                self.gate_shared(ui::row(theme, "tabs-position", "Position"))
                    .control(ui::segmented(
                        theme,
                        "tabs-position-control",
                        &[
                            (TabBarPosition::Top, "Top"),
                            (TabBarPosition::Bottom, "Bottom"),
                        ],
                        position,
                        ready,
                        on_value(cx, |this, choice, _, cx| {
                            this.save_shared(Edit::TabBarPosition(choice), cx)
                        }),
                    )),
            )
            .child(
                self.gate_shared(ui::row(theme, "tabs-hide-single", "Hide with a single tab"))
                    .switch(
                        ui::toggle(theme, "tabs-hide-single-toggle", hide, ready).when(
                            ready,
                            |toggle| {
                                toggle.on_click(cx.listener(move |this, _, _, cx| {
                                    this.save_shared(Edit::HideSingleTabBar(!hide), cx)
                                }))
                            },
                        ),
                    ),
            )
            .child(ui::section(theme, "Closing", None))
            .child(
                ui::row(theme, "tabs-confirm-tab", "Confirm before closing a tab").switch(
                    self.preference_toggle(
                        "tabs-confirm-tab-toggle",
                        self.config.confirm_close_tab,
                        Preference::ConfirmCloseTab(!self.config.confirm_close_tab),
                        cx,
                    ),
                ),
            )
            .child(
                ui::row(theme, "tabs-confirm-pane", "Confirm before closing a pane").switch(
                    self.preference_toggle(
                        "tabs-confirm-pane-toggle",
                        self.config.confirm_close_pane,
                        Preference::ConfirmClosePane(!self.config.confirm_close_pane),
                        cx,
                    ),
                ),
            )
            .child(
                ui::row(
                    theme,
                    "tabs-confirm-workspace",
                    "Confirm before closing a workspace",
                )
                .switch(self.preference_toggle(
                    "tabs-confirm-workspace-toggle",
                    self.config.confirm_close_workspace,
                    Preference::ConfirmCloseWorkspace(!self.config.confirm_close_workspace),
                    cx,
                )),
            )
            .child(ui::section(theme, "Panes", None))
            .child(
                ui::row(theme, "tabs-focus-follows-mouse", "Focus follows mouse")
                    .description("The pointer chooses the active pane within a split terminal tab.")
                    .switch(self.preference_toggle(
                        "tabs-focus-follows-mouse-toggle",
                        self.config.focus_follows_mouse,
                        Preference::FocusFollowsMouse(!self.config.focus_follows_mouse),
                        cx,
                    )),
            )
            .child(ui::section(theme, "Session restore", None))
            .child(
                self.gate_shared(
                    ui::row(
                        theme,
                        "tabs-pane-history",
                        "Restore scrollback after Herdr restarts",
                    )
                    .tag(ui::Source::Herdr)
                    .description(
                        "Experimental. Herdr saves pane output to session-history.json and \
                         replays it when the daemon restarts.",
                    ),
                )
                .switch(
                    ui::toggle(theme, "tabs-pane-history-toggle", history, ready).when(
                        ready,
                        |toggle| {
                            toggle.on_click(cx.listener(move |this, _, _, cx| {
                                this.save_shared(Edit::PaneHistory(!history), cx)
                            }))
                        },
                    ),
                ),
            );
        for (index, host) in self.remote_history.hosts().iter().enumerate() {
            let (checked, settled) = match &host.state {
                HostState::Ready(history) => (history.enabled, true),
                HostState::Saving(history) => (history.enabled, false),
                HostState::Loading | HostState::Failed(_) => (false, false),
            };
            let enabled = open && settled;
            let target = host.target.clone();
            let note = match &host.state {
                HostState::Loading => "Reading host config\u{2026}".to_owned(),
                HostState::Saving(_) => "Saving host config\u{2026}".to_owned(),
                HostState::Failed(error) => {
                    format!("{error}. Reload configuration to try again.")
                }
                HostState::Ready(_) => "Each SSH host keeps its own setting.".to_owned(),
            };
            let id = format!("tabs-pane-history-host-{index}");
            page = page.child(
                ui::row(theme, id.clone(), host.label.clone())
                    .description(note)
                    .switch(
                        ui::toggle(theme, format!("{id}-toggle"), checked, enabled).when(
                            enabled,
                            |toggle| {
                                toggle.on_click(cx.listener(move |this, _, _, cx| {
                                    this.save_remote_history(&target, !checked, cx);
                                }))
                            },
                        ),
                    ),
            );
        }
        page
    }
}
