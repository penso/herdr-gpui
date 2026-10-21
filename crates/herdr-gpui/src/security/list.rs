//! The list beside an open page: the same items as the section's cards, in
//! the same order, one compact row each, the open one marked. It keeps the
//! room the page's pane leaves, as an issue list does.

use super::{
    Item,
    page::Opened,
    section::{ago, severity_color},
};
use crate::HerdrWindow;
use gpui::{prelude::*, *};
use std::time::SystemTime;

impl HerdrWindow {
    pub(super) fn render_security_list(
        &self,
        opened: &Opened,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let theme = &self.theme;
        let mut list = div()
            .id("security-list")
            .debug_selector(|| "security-list".into())
            .flex_1()
            .min_w_0()
            .h_full()
            .min_h_0()
            .overflow_y_scroll()
            .track_scroll(&self.security.scroll)
            .flex()
            .flex_col()
            .pb_3()
            .border_r_1()
            .border_color(rgb(theme.active));
        let look = self.security_look();
        let reported = self.shown_reports();
        if !reported.is_empty() {
            list = list.child(
                self.security_group("Reported to you", format!("{} in triage", reported.len())),
            );
            let now = SystemTime::now();
            for (index, repo, report) in reported {
                let color = report
                    .severity
                    .map_or(theme.muted, |severity| severity_color(theme, severity));
                let detail = [
                    Some(repo.name.clone()),
                    report
                        .reporter
                        .as_ref()
                        .map(|login| format!("reported by {login}")),
                    report
                        .created_at
                        .map(|at| ago(now.duration_since(at.into()).unwrap_or_default())),
                ]
                .into_iter()
                .flatten()
                .collect::<Vec<_>>()
                .join(" · ");
                let item = Item::Report {
                    repo: repo.name.clone(),
                    advisory: report.advisory.clone(),
                };
                let start =
                    SharedString::from(format!("security-list-start-{index}-{}", report.advisory));
                let start = self.security_start(&look, start, item.clone(), cx);
                list = list.child(
                    self.security_row(item, opened, color, report.summary.clone(), detail, cx)
                        .child(start),
                );
            }
        }
        for (index, repo) in self.security.repos().iter().enumerate() {
            let alerts = self.shown_alerts(repo);
            if alerts.is_empty() {
                continue;
            }
            list = list
                .child(self.security_group(repo.name.clone(), format!("{} open", alerts.len())));
            for alert in alerts {
                let item = Item::Alert {
                    repo: repo.name.clone(),
                    number: alert.number,
                };
                let start =
                    SharedString::from(format!("security-list-start-{index}-{}", alert.number));
                let start = self.security_start(&look, start, item.clone(), cx);
                list = list.child(
                    self.security_row(
                        item,
                        opened,
                        severity_color(theme, alert.severity),
                        format!("{} — {}", alert.package, alert.summary),
                        format!(
                            "#{} · {} · {}",
                            alert.number, alert.advisory, alert.manifest
                        ),
                        cx,
                    )
                    .child(start),
                );
            }
        }
        list
    }

    fn security_group(&self, name: impl Into<SharedString>, count: String) -> Div {
        let theme = &self.theme;
        div()
            .px_3()
            .pt_3()
            .pb_1()
            .flex()
            .items_center()
            .gap_2()
            .min_w_0()
            .text_size(px(self.config.ui.size - 1.))
            .child(
                div()
                    .min_w_0()
                    .truncate()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(rgb(theme.subtext()))
                    .child(name.into()),
            )
            .child(div().flex_none().text_color(rgb(theme.muted)).child(count))
    }

    fn security_row(
        &self,
        item: Item,
        opened: &Opened,
        color: u32,
        title: String,
        detail: String,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let theme = &self.theme;
        let selected = item == opened.item;
        let hover = theme.active;
        let id = match &item {
            Item::Alert { repo, number } => format!("security-row-{repo}#{number}"),
            Item::Report { repo, advisory } => format!("security-row-{repo}/{advisory}"),
        };
        div()
            .id(SharedString::from(id))
            .debug_selector(move || {
                if selected {
                    "security-row-selected".into()
                } else {
                    "security-row".into()
                }
            })
            .px_3()
            .py(px(6.))
            .flex()
            .gap_2()
            .border_l_2()
            .border_color(rgb(if selected {
                theme.primary()
            } else {
                theme.background
            }))
            .when(selected, |row| row.bg(rgb(theme.primary_wash())))
            .when(!selected, |row| {
                row.cursor_pointer()
                    .hover(move |style| style.bg(rgb(hover)))
            })
            .child(
                div()
                    .flex_none()
                    .mt(px(5.))
                    .size(px(8.))
                    .rounded_full()
                    .bg(rgb(color)),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(px(2.))
                    .child(div().truncate().child(title))
                    .child(
                        div()
                            .truncate()
                            .text_size(px(self.config.ui.size - 1.))
                            .text_color(rgb(theme.muted))
                            .child(detail),
                    ),
            )
            .on_click(cx.listener(move |this, _, _, cx| {
                this.open_security_item(item.clone(), cx);
            }))
    }
}
