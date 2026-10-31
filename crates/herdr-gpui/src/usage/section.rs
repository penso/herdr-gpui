//! The activity bar's Usage section: every host's plan limits across the
//! window's body, one card per signed-in agent, drawn from the same reports
//! as the status bar's usage panel. Only the selected host is read; another
//! host shows what was read while it was selected.

use super::{Host, text, ui::Ui};
use crate::{HerdrWindow, i18n::fl};
use gpui::{prelude::*, *};
use std::time::{Instant, SystemTime};

const CARD_WIDTH: f32 = 320.;

impl HerdrWindow {
    pub(crate) fn render_usage_section(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = &self.theme;
        let font = &self.config.ui;
        let small = px(font.size - 1.);
        let now = SystemTime::now();
        let header = div()
            .debug_selector(|| "usage-section-header".into())
            .flex_none()
            .px_5()
            .py_3()
            .flex()
            .items_center()
            .gap_3()
            .border_b_1()
            .border_color(rgb(theme.active))
            .child(
                div()
                    .text_size(px(font.size + 3.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(fl!("usage-section-title")),
            )
            .child(div().flex_1())
            .when(self.config.usage.show, |header| {
                header.child(
                    div()
                        .id("usage-section-refresh")
                        .debug_selector(|| "usage-section-refresh".into())
                        .px(px(10.))
                        .h(px(24.))
                        .flex()
                        .items_center()
                        .gap_1()
                        .rounded(px(crate::config::corners::CONTROL))
                        .border_1()
                        .border_color(rgb(theme.active))
                        .cursor_pointer()
                        .hover(|style| style.bg(rgb(theme.active)))
                        .text_size(small)
                        .child(
                            svg()
                                .path("icons/refresh.svg")
                                .size(px(12.))
                                .text_color(rgb(theme.subtext())),
                        )
                        .child(if self.usage.busy() {
                            fl!("usage-updating")
                        } else {
                            fl!("usage-refresh")
                        })
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.usage.refresh(Instant::now());
                            cx.notify();
                        })),
                )
            });
        let mut body = div().p_5().flex().flex_col().gap_5();
        if !self.config.usage.show {
            body = body.child(
                div()
                    .debug_selector(|| "usage-section-off".into())
                    .text_color(rgb(theme.muted))
                    .child(fl!("usage-off")),
            );
        }
        let ui = Ui {
            theme: self.theme.clone(),
            font_size: font.size,
            now,
        };
        for (index, endpoint) in self.endpoints.iter().enumerate() {
            let Some(host) = Host::of(&endpoint.connection.target).filter(|_| endpoint.enabled)
            else {
                continue;
            };
            if !self.config.usage.show {
                break;
            }
            body = body.child(self.usage_host(index, &endpoint.label, &host, &ui));
        }
        div()
            .id("usage-section")
            .debug_selector(|| "usage-section".into())
            .flex()
            .flex_col()
            .flex_1()
            .size_full()
            .min_w_0()
            .min_h_0()
            .bg(rgb(theme.background))
            .text_color(rgb(theme.foreground))
            .text_size(px(font.size))
            .child(header)
            .child(
                div()
                    .id("usage-section-scroll")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .child(body),
            )
            .into_any_element()
    }

    fn usage_host(&self, index: usize, label: &str, host: &Host, ui: &Ui) -> Div {
        let theme = &self.theme;
        let small = ui.small();
        let selected = index == self.selected_endpoint;
        let entry = self.usage.entry(host);
        let updated = entry
            .and_then(|entry| entry.updated)
            .map(|at| text::updated(ui.now.duration_since(at).unwrap_or_default()))
            .unwrap_or_else(|| {
                if selected {
                    fl!("usage-not-read-yet")
                } else {
                    fl!("usage-read-while-selected")
                }
            });
        let readings = entry
            .map(|entry| entry.tabs(&self.config.usage))
            .unwrap_or_default();
        let selector = format!("usage-section-host-{index}");
        let mut cards = div().flex().flex_wrap().gap_3();
        if readings.is_empty() {
            cards = cards.child(
                div()
                    .text_color(rgb(theme.muted))
                    .child(fl!("usage-no-agents")),
            );
        }
        for reading in readings {
            let service = reading.provider.service();
            let account = reading
                .report
                .as_ref()
                .map(|report| report.account.clone())
                .unwrap_or_default();
            let mut card = div()
                .w(px(CARD_WIDTH))
                .flex_none()
                .p_2()
                .flex()
                .flex_col()
                .gap_1()
                .rounded(px(crate::config::corners::PANEL))
                .bg(rgb(theme.surface))
                .child(
                    div()
                        .px(px(8.))
                        .pt(px(4.))
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(
                            div()
                                .flex_1()
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(service.meta().name),
                        )
                        .children(account.plan.map(|plan| {
                            div()
                                .text_size(small)
                                .text_color(rgb(theme.muted))
                                .child(plan)
                        })),
                );
            if let Some(error) = reading.error.clone() {
                card = card.child(
                    div()
                        .px(px(8.))
                        .text_size(small)
                        .text_color(rgb(theme.ink(theme.palette[3])))
                        .child(error),
                );
            }
            if let Some(report) = reading.report.as_ref() {
                card = card.child(super::details::render(report, ui));
            }
            cards = cards.child(card);
        }
        div()
            .debug_selector(move || selector.clone())
            .flex()
            .flex_col()
            .gap_2()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        svg()
                            .path("icons/computer.svg")
                            .size(px(13.))
                            .text_color(rgb(theme.subtext())),
                    )
                    .child(
                        div()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(label.to_owned()),
                    )
                    .child(
                        div()
                            .text_size(small)
                            .text_color(rgb(theme.muted))
                            .child(updated),
                    )
                    .when_some(entry.and_then(|entry| entry.error.clone()), |row, error| {
                        row.child(
                            div()
                                .text_size(small)
                                .text_color(rgb(theme.ink(theme.palette[3])))
                                .child(error),
                        )
                    }),
            )
            .child(cards)
    }
}
