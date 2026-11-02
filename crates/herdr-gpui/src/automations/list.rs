//! The list beside an open script or run: every script, grouped by machine,
//! one compact row each, the open one marked.

use super::{HostState, Opened, Read, model};
use crate::{HerdrWindow, i18n::fl};
use gpui::{prelude::*, *};

/// The list's width beside a page.
const WIDTH: f32 = 280.;

impl HerdrWindow {
    pub(super) fn render_automation_list(
        &self,
        opened: &Opened,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let theme = &self.theme;
        let mut list = div()
            .id("automations-list")
            .debug_selector(|| "automations-list".into())
            .flex_none()
            .w(px(WIDTH))
            .h_full()
            .min_h_0()
            .overflow_y_scroll()
            .track_scroll(&self.automations.scroll)
            .flex()
            .flex_col()
            .pb_3()
            .border_r_1()
            .border_color(rgb(theme.active));
        for host in &self.automations.hosts {
            list = list.child(self.automation_list_host(host));
            let Some(status) = host.status() else {
                continue;
            };
            for script in &status.scripts {
                let selected = *opened.machine() == host.machine && opened.path() == script.path;
                let target = Opened::Script {
                    machine: host.machine.clone(),
                    path: script.path.clone(),
                };
                let hover = theme.active;
                let detail = fl!(
                    "automations-row-detail",
                    schedule = model::schedule(script),
                    tokens = model::tokens_today(script)
                );
                list = list.child(
                    div()
                        .id(SharedString::from(format!(
                            "automations-row-{}-{}",
                            host.label, script.path
                        )))
                        .debug_selector(move || {
                            if selected {
                                "automations-row-selected".into()
                            } else {
                                "automations-row".into()
                            }
                        })
                        .px_3()
                        .py(px(5.))
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
                                .pt(px(2.))
                                .child(self.automation_icon(model::look(script), 13.)),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .flex()
                                .flex_col()
                                .child(div().truncate().child(script.name.clone()))
                                .child(
                                    div()
                                        .truncate()
                                        .text_size(self.automation_small())
                                        .text_color(rgb(theme.muted))
                                        .child(detail),
                                ),
                        )
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.open_automation(target.clone(), cx);
                        })),
                );
            }
        }
        list
    }

    fn automation_list_host(&self, host: &HostState) -> Div {
        let theme = &self.theme;
        let state = match &host.read {
            Read::Missing => Some((fl!("automations-host-missing"), theme.muted)),
            Read::Failed { .. } => Some((
                fl!("automations-host-unreachable"),
                theme.ink(theme.palette[3]),
            )),
            Read::Ready(status) if !status.daemon.running => Some((
                fl!("automations-host-daemon-down"),
                theme.ink(theme.palette[3]),
            )),
            Read::Ready(_) if self.automation_update(host).is_some() => {
                Some((fl!("automations-host-update"), theme.ink(theme.palette[3])))
            }
            Read::Ready(_) | Read::Unread => None,
        };
        div()
            .px_3()
            .pt_3()
            .pb_1()
            .flex()
            .items_center()
            .gap_2()
            .min_w_0()
            .text_size(self.automation_small())
            .text_color(rgb(theme.muted))
            .child(
                svg()
                    .flex_none()
                    .size(px(13.))
                    .path(host.machine.icon())
                    .text_color(rgb(theme.subtext())),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(rgb(theme.subtext()))
                    .child(host.label.clone()),
            )
            .children(state.map(|(text, color)| self.automation_chip(text, color)))
    }
}
