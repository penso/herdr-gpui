//! Theme > Window and Terminal padding: what shows behind the main window,
//! and the space between each terminal and the edges of its tab group.
use super::{SettingsWindow, on_value, ui};
use crate::{
    config::{Axis, Side, preferences::Preference},
    settings_window::inputs::NumberField,
};
use gpui::{prelude::*, *};

/// The four padded sides, in the order the page lists them.
pub(in crate::settings_window) const PADDING_SIDES: [(Axis, Side); 4] = [
    (Axis::X, Side::Start),
    (Axis::X, Side::End),
    (Axis::Y, Side::Start),
    (Axis::Y, Side::End),
];

/// The row id, the number field's id and the title of one padded side.
pub(in crate::settings_window) fn padding_ids(
    axis: Axis,
    side: Side,
) -> (&'static str, &'static str, &'static str) {
    match (axis, side) {
        (Axis::X, Side::Start) => ("theme-padding-left", "terminal-padding-left", "Left"),
        (Axis::X, Side::End) => ("theme-padding-right", "terminal-padding-right", "Right"),
        (Axis::Y, Side::Start) => ("theme-padding-top", "terminal-padding-top", "Top"),
        (Axis::Y, Side::End) => ("theme-padding-bottom", "terminal-padding-bottom", "Bottom"),
    }
}

impl SettingsWindow {
    pub(in crate::settings_window) fn render_window_rows(&self, cx: &mut Context<Self>) -> Div {
        let theme = &self.theme;
        div()
            .flex()
            .flex_col()
            .child(
                ui::section(theme, "Window", Some(ui::Source::HerdrUi))
                    .debug_selector(|| "settings-window-section".into()),
            )
            .child(self.render_backdrop_controls(cx))
            .child(
                ui::section(theme, "Terminal padding", Some(ui::Source::HerdrUi))
                    .debug_selector(|| "settings-terminal-padding".into()),
            )
            .children(PADDING_SIDES.map(|(axis, side)| self.padding_row(axis, side, cx)))
    }

    fn padding_row(&self, axis: Axis, side: Side, cx: &mut Context<Self>) -> ui::Row {
        let field = NumberField::TerminalPadding(axis, side);
        let (row, _, title) = padding_ids(axis, side);
        let value = self.number_value(field, self.config.terminal_padding.axis(axis).side(side));
        let view = cx.entity().downgrade();
        ui::row(&self.theme, row, title)
            .when(axis == Axis::X && side == Side::Start, |row| {
                row.description(
                    "Beside the sidebar. A deprecated sidebar_gap stands in while unset.",
                )
            })
            .control(ui::number(
                &self.theme,
                ui::Number {
                    id: field.selector(),
                    value,
                    range: field.range(),
                    unit: "px",
                    look: ui::Look::Slider,
                    enabled: true,
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
            ))
    }

    /// Saves one side; the other side of its axis is written along with it,
    /// since the file keeps them together.
    pub(in crate::settings_window) fn set_terminal_padding(
        &mut self,
        axis: Axis,
        side: Side,
        value: f32,
        cx: &mut Context<Self>,
    ) {
        let sides = self.config.terminal_padding.axis(axis);
        let moved = sides.stepped(side, value - sides.side(side));
        self.save_preference(Preference::TerminalPadding(axis, moved), cx);
    }
}
