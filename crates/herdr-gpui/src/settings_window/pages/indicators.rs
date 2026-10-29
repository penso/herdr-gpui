//! Indicators: the set agent status is drawn with, as cards to click, and the
//! edge rail. Each card's preview is the sidebar's own marks, so a mark that
//! moves there moves here too.
// i18n: migrated
use super::{SettingsWindow, ui};
use crate::{
    config::{StatusIcons, corners, preferences::Preference},
    i18n::{self, fl},
    settings_window::controls::indicators::set_messages,
    sidebar::{Indicators, status_indicator},
};
use gpui::{prelude::*, *};
use herdr_client::protocol::AgentStatus;

/// The statuses each card previews, most urgent first, with the messages
/// naming them.
const PREVIEW: [(AgentStatus, &str); 5] = [
    (AgentStatus::Working, "settings-indicators-status-working"),
    (AgentStatus::Blocked, "settings-indicators-status-blocked"),
    (AgentStatus::Done, "settings-indicators-status-done"),
    (AgentStatus::Idle, "settings-indicators-status-idle"),
    (AgentStatus::Unknown, "settings-indicators-status-unknown"),
];
const CARD_WIDTH: f32 = 212.;

impl SettingsWindow {
    pub(in crate::settings_window) fn render_indicators(&self, cx: &mut Context<Self>) -> Div {
        let theme = &self.theme;
        let rail = self.config.edge_rail;
        let light = matches!(
            cx.window_appearance(),
            WindowAppearance::Light | WindowAppearance::VibrantLight
        );
        let indicators = Indicators::new(
            self.shared.as_ref(),
            self.config.status_icons,
            rail,
            light,
            theme,
        );
        div()
            .flex()
            .flex_col()
            .child(
                ui::row(
                    theme,
                    "indicators-edge-rail",
                    fl!("settings-indicators-edge-rail"),
                )
                .tag(ui::Source::HerdrUi)
                .description(fl!("settings-indicators-edge-rail-description"))
                .switch(self.preference_toggle(
                    "indicators-edge-rail-toggle",
                    rail,
                    Preference::EdgeRail(!rail),
                    cx,
                )),
            )
            .child(ui::section(
                theme,
                &fl!("settings-indicators-sets"),
                Some(ui::Source::HerdrUi),
            ))
            .child(
                div()
                    .relative()
                    .debug_selector(|| "indicators-sets".into())
                    // Search scrolls to the cards by these bounds.
                    .child(crate::settings_window::search::track_bounds(
                        "indicators-sets".into(),
                    ))
                    .flex()
                    .flex_wrap()
                    .gap(px(10.))
                    .pt(px(4.))
                    .children(
                        StatusIcons::ALL
                            .iter()
                            .map(|&set| self.set_card(set, indicators.with_icons(set), cx)),
                    ),
            )
    }

    /// One set as a card: its name and a radio mark, the five statuses drawn
    /// with it, and a line on what sets it apart. Clicking picks it.
    fn set_card(
        &self,
        set: StatusIcons,
        indicators: Indicators,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let theme = &self.theme;
        let on = self.config.status_icons == set;
        let font = &self.config.sidebar;
        let (name, description) = set_messages(set);
        div()
            .id(("indicator-set", set as usize))
            .debug_selector(move || format!("indicator-set-{}", set.as_str()))
            .w(px(CARD_WIDTH))
            .flex()
            .flex_col()
            .gap(px(8.))
            .p(px(10.))
            .rounded(px(corners::CONTROL))
            .border_1()
            .border_color(rgb(if on { theme.primary() } else { theme.active }))
            .bg(rgb(theme.surface))
            .when(!on, |card| {
                card.cursor_pointer()
                    .hover(|card| card.border_color(rgb(theme.muted)))
                    .on_click(cx.listener(move |this, _, _, cx| this.choose_status_icons(set, cx)))
            })
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .text_color(rgb(theme.foreground))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(i18n::message(name, &[]))
                    .child(radio(theme, on)),
            )
            .child(
                div()
                    .flex()
                    .justify_between()
                    .px(px(4.))
                    .py(px(6.))
                    .rounded(px(corners::SMALL))
                    .bg(rgb(theme.background))
                    .children(PREVIEW.map(|(status, message)| {
                        div()
                            .flex()
                            .flex_col()
                            .items_center()
                            .gap(px(3.))
                            .child(status_indicator(status, font, indicators))
                            .child(
                                div()
                                    .text_size(px(9.5))
                                    .text_color(rgb(theme.muted))
                                    .child(i18n::message(message, &[])),
                            )
                    })),
            )
            .child(
                div()
                    .text_size(px(11.5))
                    .text_color(rgb(theme.subtext()))
                    .child(i18n::message(description, &[])),
            )
    }
}

fn radio(theme: &crate::config::Theme, on: bool) -> Div {
    div()
        .size(px(14.))
        .flex()
        .items_center()
        .justify_center()
        .rounded_full()
        .border_1()
        .border_color(rgb(if on { theme.primary() } else { theme.muted }))
        .when(on, |radio| {
            radio.child(div().size(px(6.)).rounded_full().bg(rgb(theme.primary())))
        })
}

#[cfg(test)]
mod tests;
