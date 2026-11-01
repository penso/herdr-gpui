//! One row of the Ports section: the port and where it listens; what runs
//! it, its project and directory, and its command line; its CPU and
//! memory; and under the name, Open, Pause or Resume, and Stop, which wrap
//! rather than overflow a narrow window.
// i18n: migrated

use super::{Listener, Pending, home_relative, section::uptime_text};
use crate::{
    HerdrWindow,
    config::corners,
    direction::Logical,
    i18n::fl,
    pane_menu::{cpu_text, memory_text},
};
use gpui::{prelude::*, *};
use herdr_control_protocol::protocol::hosts::PortAction;
use std::{path::Path, time::SystemTime};

impl HerdrWindow {
    pub(super) fn local_port_row(
        &self,
        index: usize,
        listener: &Listener,
        now: SystemTime,
        home: Option<&Path>,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let theme = &self.theme;
        let small = px(self.config.ui.size - 1.);
        let owner = self.local_port_owner(listener.number);
        let pill = |text: SharedString, color: u32| {
            div()
                .flex_none()
                .px(px(6.))
                .rounded(px(corners::SMALL))
                .bg(rgb(theme.active))
                .text_size(small)
                .text_color(rgb(color))
                .whitespace_nowrap()
                .child(text)
        };
        let place = listener.directory.as_deref().map(|path| {
            let project = listener
                .project
                .clone()
                .or_else(|| Some(path.file_name()?.to_string_lossy().into_owned()))
                .unwrap_or_else(|| home_relative(path, home));
            fl!(
                "local-ports-directory",
                project = project.as_str(),
                path = home_relative(path, home)
            )
        });
        let detail = if listener.command.is_empty() {
            listener.process.clone()
        } else {
            listener.command.clone()
        };
        let selector = format!("local-port-{index}");
        let usage_selector = format!("local-port-{index}-usage");
        div()
            .id(("local-port", index))
            .debug_selector(move || selector.clone())
            .px_3()
            .py_2()
            .row()
            .items_start()
            .gap_3()
            .rounded(px(corners::PANEL))
            .bg(rgb(theme.surface))
            .child(
                div()
                    .flex_none()
                    .w(px(96.))
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(format!(":{}", listener.number)),
                    )
                    .child(
                        div()
                            .text_size(small)
                            .text_color(rgb(theme.muted))
                            .truncate()
                            .child(listener.address()),
                    ),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .row()
                            .items_center()
                            .gap_2()
                            .min_w_0()
                            .child(
                                div()
                                    .min_w_0()
                                    .truncate()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .when(listener.system, |name| {
                                        name.text_color(rgb(theme.subtext()))
                                    })
                                    .child(listener.name.clone()),
                            )
                            .child(
                                div()
                                    .flex_none()
                                    .text_size(small)
                                    .text_color(rgb(theme.muted))
                                    .child(fl!("local-ports-pid", pid = listener.identity.pid)),
                            )
                            .when(listener.paused, |line| {
                                line.child(pill(
                                    fl!("local-ports-paused").into(),
                                    theme.ink(theme.palette[3]),
                                ))
                            })
                            .children(owner.map(|owned| {
                                pill(
                                    fl!("local-ports-workspace", workspace = owned.label.as_str())
                                        .into(),
                                    theme.subtext(),
                                )
                            })),
                    )
                    .child(
                        div()
                            .row()
                            .items_center()
                            .gap_2()
                            .min_w_0()
                            .text_size(small)
                            .text_color(rgb(theme.muted))
                            .children(place.map(|place| div().min_w_0().truncate().child(place)))
                            .children(
                                listener
                                    .uptime(now)
                                    .map(|uptime| div().flex_none().child(uptime_text(uptime))),
                            ),
                    )
                    .child(
                        div()
                            .min_w_0()
                            .truncate()
                            .text_size(small)
                            .text_color(rgb(theme.muted))
                            .child(detail),
                    )
                    .child(self.local_port_actions(index, listener, cx)),
            )
            .child(
                div()
                    .debug_selector(move || usage_selector.clone())
                    .flex_none()
                    .row()
                    .justify_end()
                    .gap_3()
                    .text_size(small)
                    .text_color(rgb(theme.subtext()))
                    .child(
                        div()
                            .w(px(44.))
                            .row()
                            .justify_end()
                            .child(cpu_text(listener.cpu)),
                    )
                    .child(
                        div()
                            .w(px(64.))
                            .row()
                            .justify_end()
                            .child(memory_text(listener.memory)),
                    ),
            )
    }

    fn local_port_actions(&self, index: usize, listener: &Listener, cx: &mut Context<Self>) -> Div {
        let action = |id: &str, label: SharedString| {
            self.local_ports_button(format!("local-port-{index}-{id}").into(), label)
        };
        let mut actions = div().pt_1().row().flex_wrap().items_center().gap_1();
        if listener.url().is_some() {
            let opened = listener.clone();
            actions = actions.child(action("open", fl!("local-ports-open").into()).on_click(
                cx.listener(move |this, _, window, cx| {
                    this.open_listener_page(&opened, window, cx)
                }),
            ));
        }
        if self.local_ports.acting() {
            return actions;
        }
        let chosen = listener.clone();
        actions = actions.child(if listener.paused {
            action("resume", fl!("local-ports-resume").into())
                .on_click(cx.listener(move |this, _, _, cx| this.resume_local_port(&chosen, cx)))
        } else {
            action("pause", fl!("local-ports-pause").into()).on_click(cx.listener(
                move |this, _, window, cx| {
                    this.ask_to_signal_ports(
                        Pending {
                            action: PortAction::Pause,
                            listeners: vec![chosen.clone()],
                        },
                        window,
                        cx,
                    );
                },
            ))
        });
        let chosen = listener.clone();
        actions.child(
            action("stop", fl!("local-ports-stop").into())
                .text_color(crate::menu::danger(&self.theme))
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.ask_to_signal_ports(
                        Pending {
                            action: PortAction::Stop,
                            listeners: vec![chosen.clone()],
                        },
                        window,
                        cx,
                    );
                })),
        )
    }
}
