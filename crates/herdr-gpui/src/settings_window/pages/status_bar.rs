//! Status bar: a switch per item, and how compactly a shown item draws.
//! Usage, CPU/memory and network quality keep their existing visibility keys,
//! which also stop their sampling.
// i18n: migrated
use super::{SettingsWindow, on_value, ui};
use crate::{
    config::{
        Config,
        preferences::Preference,
        status_bar::{AwakeScope, Button, Detail, Edit},
    },
    i18n::{self, fl},
};
use gpui::{prelude::*, *};

/// Each choice of a segmented control, with the message naming it.
const DETAILS: [(Detail, &str); 2] = [
    (Detail::Detailed, "settings-status-bar-detailed"),
    (Detail::Compact, "settings-status-bar-compact"),
];
const SCOPES: [(AwakeScope, &str); 2] = [
    (AwakeScope::Display, "settings-status-bar-awake-display"),
    (AwakeScope::System, "settings-status-bar-awake-system"),
];
const STYLES: [(Button, &str); 2] = [
    (Button::Label, "settings-status-bar-with-label"),
    (Button::Icon, "settings-status-bar-icon-only"),
];

/// The choices of a segmented control in the current locale.
fn choices<T: Copy>(choices: &[(T, &str)]) -> Vec<(T, String)> {
    choices
        .iter()
        .map(|&(value, message)| (value, i18n::message(message, &[])))
        .collect()
}

/// A status bar button row: its id, the message titling it, current value
/// and edit.
struct ButtonRow {
    id: &'static str,
    toggle: &'static str,
    style: &'static str,
    title: &'static str,
    current: Button,
    edit: fn(Button) -> Edit,
}

impl SettingsWindow {
    pub(in crate::settings_window) fn render_status_bar(&self, cx: &mut Context<Self>) -> Div {
        let theme = &self.theme;
        let items = self.config.status_bar;
        let usage = self.config.usage.show;
        let system_load = self.config.show_system_load;
        let network_quality = self.config.show_network_quality;
        let gpu = self.config.show_gpu_load;
        let keep_awake = items.keep_awake;
        let buttons = [
            ButtonRow {
                id: "status-theme",
                toggle: "status-theme-toggle",
                style: "status-theme-style",
                title: "settings-status-theme",
                current: items.theme,
                edit: Edit::Theme,
            },
            ButtonRow {
                id: "status-shortcuts",
                toggle: "status-shortcuts-toggle",
                style: "status-shortcuts-style",
                title: "settings-status-shortcuts",
                current: items.shortcuts,
                edit: Edit::Shortcuts,
            },
            ButtonRow {
                id: "status-report-issue",
                toggle: "status-report-issue-toggle",
                style: "status-report-issue-style",
                title: "settings-status-report-issue",
                current: items.report_issue,
                edit: Edit::ReportIssue,
            },
        ];
        div()
            .flex()
            .flex_col()
            .child(ui::section(
                theme,
                &fl!("settings-status-bar-section-items"),
                Some(ui::Source::HerdrUi),
            ))
            .child(
                ui::row(theme, "status-usage", fl!("settings-status-usage"))
                    .description(fl!("settings-status-bar-usage-hint"))
                    .control(self.detail_control(
                        "status-usage-detail",
                        items.usage,
                        usage,
                        Edit::Usage,
                        cx,
                    ))
                    .switch(
                        ui::toggle(theme, "status-usage-toggle", usage, true).on_click(
                            cx.listener(move |this, _, _, cx| {
                                this.save_native(move || Config::save_usage_visibility(!usage), cx);
                            }),
                        ),
                    ),
            )
            .child(
                ui::row(
                    theme,
                    "status-system-load",
                    fl!("settings-status-system-load"),
                )
                .description(fl!("settings-status-system-load-description"))
                .control(self.detail_control(
                    "status-system-load-detail",
                    items.system_load,
                    system_load,
                    Edit::SystemLoad,
                    cx,
                ))
                .switch(self.preference_toggle(
                    "status-system-load-toggle",
                    system_load,
                    Preference::ShowSystemLoad(!system_load),
                    cx,
                )),
            )
            .child(
                ui::row(theme, "status-gpu", fl!("settings-status-gpu"))
                    .description(fl!("settings-status-bar-gpu-hint"))
                    .control(self.detail_control(
                        "status-gpu-detail",
                        items.gpu_detail,
                        gpu,
                        Edit::GpuDetail,
                        cx,
                    ))
                    .switch(self.preference_toggle(
                        "status-gpu-toggle",
                        gpu,
                        Preference::ShowGpuLoad(!gpu),
                        cx,
                    )),
            )
            .child(
                ui::row(
                    theme,
                    "status-network-quality",
                    fl!("settings-status-network-quality"),
                )
                .description(fl!("settings-status-network-quality-description"))
                .switch(self.preference_toggle(
                    "status-network-quality-toggle",
                    network_quality,
                    Preference::ShowNetworkQuality(!network_quality),
                    cx,
                )),
            )
            .child(
                ui::row(
                    theme,
                    "status-keep-awake",
                    fl!("settings-status-keep-awake"),
                )
                .description(fl!("settings-status-keep-awake-description"))
                .control(ui::segmented(
                    theme,
                    "status-keep-awake-scope",
                    &choices(&SCOPES),
                    Some(items.keep_awake_scope),
                    true,
                    on_value(cx, |this, scope, _, cx| {
                        this.save_preference(Preference::StatusBar(Edit::KeepAwakeScope(scope)), cx)
                    }),
                ))
                .switch(self.preference_toggle(
                    "status-keep-awake-toggle",
                    keep_awake,
                    Preference::StatusBar(Edit::KeepAwake(!keep_awake)),
                    cx,
                )),
            )
            .children(buttons.map(|button| self.button_row(button, cx)))
            .child(
                self.control_note(fl!("settings-status-bar-note"))
                    .mt(px(10.))
                    .text_size(px(11.)),
            )
    }

    /// Detailed or Compact for an item; inert while the item is hidden.
    fn detail_control(
        &self,
        id: &'static str,
        current: Detail,
        shown: bool,
        edit: fn(Detail) -> Edit,
        cx: &mut Context<Self>,
    ) -> Div {
        ui::segmented(
            &self.theme,
            id,
            &choices(&DETAILS),
            shown.then_some(current),
            shown,
            on_value(cx, move |this, detail, _, cx| {
                this.save_preference(Preference::StatusBar(edit(detail)), cx)
            }),
        )
    }

    /// A switch that hides the button, then whether it keeps its name.
    fn button_row(&self, button: ButtonRow, cx: &mut Context<Self>) -> ui::Row {
        let ButtonRow {
            id,
            toggle,
            style,
            title,
            current,
            edit,
        } = button;
        let shown = current.shown();
        let toggled = if shown { Button::Hidden } else { Button::Label };
        ui::row(&self.theme, id, i18n::message(title, &[]))
            .control(ui::segmented(
                &self.theme,
                style,
                &choices(&STYLES),
                shown.then_some(current),
                shown,
                on_value(cx, move |this, choice, _, cx| {
                    this.save_preference(Preference::StatusBar(edit(choice)), cx)
                }),
            ))
            .switch(self.preference_toggle(toggle, shown, Preference::StatusBar(edit(toggled)), cx))
    }
}

#[cfg(test)]
mod tests;
