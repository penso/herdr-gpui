//! Form pieces for the pages that hold forms rather than setting rows (Code,
//! Plugins, Cloud devices): a titled block under the shared section heading,
//! and a labelled switch row. Both follow the row system's look.
// i18n: migrated
use super::*;
use crate::settings_window::ui;

impl SettingsWindow {
    /// A block of form content under a section heading; no card frame.
    pub(in crate::settings_window) fn control_card(&self, title: &str) -> Div {
        div()
            .flex()
            .flex_col()
            .gap(px(10.))
            .min_w_0()
            .child(ui::section(&self.theme, title, None))
    }

    /// A clickable row with `label` and a switch; the caller adds `.on_click`
    /// when `enabled`.
    pub(in crate::settings_window) fn control_switch(
        &self,
        id: impl Into<SharedString>,
        label: impl Into<SharedString>,
        checked: bool,
        enabled: bool,
    ) -> Stateful<Div> {
        let id = id.into();
        let toggle = format!("{id}-toggle");
        let label = label.into();
        // The whole row is the switch; the track inside only draws it.
        let row = div()
            .id(ElementId::Name(id.clone()))
            .debug_selector(move || id.to_string())
            .role(Role::Switch)
            .aria_label(label.clone())
            .aria_toggled(if checked {
                Toggled::True
            } else {
                Toggled::False
            });
        crate::accessibility::disabled(row, !enabled)
            .flex()
            .items_center()
            .justify_between()
            .gap(px(12.))
            .min_h(px(ui::ROW_HEIGHT))
            .py(px(6.))
            .border_b_1()
            .border_color(rgb(self.theme.active))
            .when(enabled, |row| row.cursor_pointer())
            .when(!enabled, |row| row.opacity(0.5))
            .child(
                div()
                    .min_w_0()
                    .text_size(px(12.))
                    .font_weight(FontWeight::MEDIUM)
                    .child(label),
            )
            .child(
                crate::toggles::switch(&self.theme, 20., checked)
                    .id(ElementId::Name(toggle.clone().into()))
                    .debug_selector(move || toggle.clone()),
            )
    }
}
