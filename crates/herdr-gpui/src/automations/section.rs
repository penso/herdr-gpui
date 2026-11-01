//! Drawing the Automations section from the last reads: a header with the
//! Hosts | Timeline toggle, today's tokens against the hosts' caps, and
//! Refresh; then either one card per machine with a row per script, or the
//! timeline. A script or run opens beside a narrowed list, as Security's
//! items do.

use super::{Action, HostState, Look, Mode, Opened, Read, format, model};
use crate::{
    HerdrWindow,
    config::corners,
    i18n::{Localize, fl},
};
use gpui::{prelude::*, *};
use herdr_automation::report::{ScriptStatus, Status};

impl HerdrWindow {
    pub(crate) fn render_automations_section(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = &self.theme;
        let font = &self.config.ui;
        let body = match self.automations.opened.clone() {
            Some(opened) => div()
                .flex_1()
                .min_h_0()
                .flex()
                .when(!self.automations.list_hidden, |split| {
                    split.child(self.render_automation_list(&opened, cx))
                })
                .child(self.render_automation_page(&opened, cx))
                .into_any_element(),
            None => div()
                .id("automations-scroll")
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .track_scroll(&self.automations.scroll)
                .child(match self.automations.mode {
                    Mode::Hosts => self.automations_hosts(cx),
                    Mode::Timeline => self.automations_timeline(cx),
                })
                .into_any_element(),
        };
        div()
            .id("automations-section")
            .debug_selector(|| "automations-section".into())
            .flex()
            .flex_col()
            .flex_1()
            .size_full()
            .min_w_0()
            .min_h_0()
            .bg(rgb(theme.background))
            .text_color(rgb(theme.foreground))
            .text_size(px(font.size))
            .child(self.automations_header(cx))
            .child(body)
            .into_any_element()
    }

    fn automations_header(&self, cx: &mut Context<Self>) -> Div {
        let theme = &self.theme;
        let font = &self.config.ui;
        let toggle = |mode: Mode, cx: &mut Context<Self>| {
            let on = self.automations.mode == mode;
            let key = mode.key();
            div()
                .id(key)
                .debug_selector(move || format!("automations-mode-{key}"))
                .px(px(10.))
                .h(px(22.))
                .flex()
                .items_center()
                .rounded(px(corners::SMALL))
                .text_size(self.automation_small())
                .cursor_pointer()
                .when(on, |el| {
                    el.bg(rgb(theme.background))
                        .font_weight(FontWeight::SEMIBOLD)
                })
                .when(!on, |el| el.text_color(rgb(theme.subtext())))
                .child(mode.title())
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.automations.mode = mode;
                    this.automations.opened = None;
                    cx.notify();
                }))
        };
        div()
            .debug_selector(|| "automations-header".into())
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
                    .child(fl!("automations-title")),
            )
            .child(
                div()
                    .p(px(2.))
                    .flex()
                    .gap(px(2.))
                    .rounded(px(corners::CONTROL))
                    .bg(rgb(theme.active))
                    .child(toggle(Mode::Hosts, cx))
                    .child(toggle(Mode::Timeline, cx)),
            )
            .child(div().flex_1())
            .children(self.automations_meter())
            .child(
                self.automation_button(
                    "automations-refresh",
                    "icons/refresh.svg",
                    if self.automations.busy() {
                        fl!("automations-updating")
                    } else {
                        fl!("automations-refresh")
                    },
                )
                .on_click(cx.listener(|this, _, _, cx| {
                    this.automations.refresh();
                    this.poll_automations(cx);
                    cx.notify();
                })),
            )
    }

    /// Today's tokens across machines against the sum of their caps.
    fn automations_meter(&self) -> Option<Div> {
        let theme = &self.theme;
        let statuses: Vec<&Status> = self
            .automations
            .hosts
            .iter()
            .filter_map(HostState::status)
            .collect();
        if statuses.is_empty() {
            return None;
        }
        let used: u64 = statuses.iter().map(|status| status.today.total()).sum();
        let cap: u64 = statuses
            .iter()
            .map(|status| status.caps.max_tokens_per_day)
            .sum();
        let share = if cap == 0 {
            0.
        } else {
            (used as f32 / cap as f32).clamp(0., 1.)
        };
        let fill = if share >= 0.9 {
            theme.ink(theme.palette[1])
        } else {
            theme.primary()
        };
        Some(
            div()
                .debug_selector(|| "automations-meter".into())
                .flex_none()
                .flex()
                .items_center()
                .gap_2()
                .text_size(self.automation_small())
                .text_color(rgb(theme.muted))
                .child(
                    div()
                        .w(px(80.))
                        .h(px(4.))
                        .rounded_full()
                        .bg(rgb(theme.active))
                        .child(
                            div()
                                .w(px(80. * share))
                                .h_full()
                                .rounded_full()
                                .bg(rgb(fill)),
                        ),
                )
                .child(fl!(
                    "automations-meter",
                    used = format::tokens(used),
                    cap = format::tokens(cap)
                )),
        )
    }

    fn automations_hosts(&self, cx: &mut Context<Self>) -> Div {
        let mut body = div().p_5().flex().flex_col().gap_4();
        if self.automations.hosts.is_empty() {
            return body.child(
                div()
                    .debug_selector(|| "automations-empty".into())
                    .text_color(rgb(self.theme.muted))
                    .child(fl!("automations-no-hosts")),
            );
        }
        for (index, host) in self.automations.hosts.iter().enumerate() {
            body = body.child(self.automation_host_card(index, host, cx));
        }
        body
    }

    fn automation_host_card(&self, index: usize, host: &HostState, cx: &mut Context<Self>) -> Div {
        let theme = &self.theme;
        let small = self.automation_small();
        let mut card = div()
            .debug_selector(move || format!("automations-host-{index}"))
            .flex()
            .flex_col()
            .rounded(px(corners::PANEL))
            .bg(rgb(theme.surface))
            .py_1()
            .child(
                div()
                    .px_3()
                    .py_2()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        svg()
                            .flex_none()
                            .size(px(14.))
                            .path(host.machine.icon())
                            .text_color(rgb(theme.subtext())),
                    )
                    .child(
                        div()
                            .flex_none()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(host.label.clone()),
                    )
                    .child(
                        div()
                            .min_w_0()
                            .truncate()
                            .text_size(small)
                            .text_color(rgb(theme.muted))
                            .child(host_detail(host)),
                    )
                    .child(div().flex_1())
                    .children(self.automation_host_controls(index, host, cx)),
            );
        let note = match &host.read {
            Read::Unread => Some((fl!("automations-reading"), theme.muted)),
            Read::Missing if host.machine.is_local() => {
                Some((fl!("automations-missing-here"), theme.muted))
            }
            Read::Missing => Some((fl!("automations-missing-host"), theme.muted)),
            Read::Failed { error, .. } => Some((error.localize(), theme.ink(theme.palette[3]))),
            Read::Ready(_) => None,
        };
        if let Some((text, color)) = note {
            card = card.child(
                div()
                    .debug_selector(move || format!("automations-host-{index}-note"))
                    .px_3()
                    .py(px(6.))
                    .border_t_1()
                    .border_color(rgb(theme.active))
                    .text_size(small)
                    .text_color(rgb(color))
                    .child(text),
            );
        }
        let Some(status) = host.status() else {
            return card;
        };
        if status.scripts.is_empty() {
            return card.child(
                div()
                    .px_3()
                    .py(px(6.))
                    .border_t_1()
                    .border_color(rgb(theme.active))
                    .text_size(small)
                    .text_color(rgb(theme.muted))
                    .child(fl!("automations-no-scripts")),
            );
        }
        let now = format::now();
        for (row, script) in status.scripts.iter().enumerate() {
            let selector = format!("automations-script-{index}-{row}");
            card = card.child(
                self.automation_script_row(host, status, script, now, cx)
                    .debug_selector(move || selector.clone()),
            );
        }
        card
    }

    /// The card's trailing controls: its health, and what can be done to
    /// the whole host.
    fn automation_host_controls(
        &self,
        index: usize,
        host: &HostState,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let theme = &self.theme;
        let mut controls = Vec::new();
        let machine = host.machine.clone();
        match &host.read {
            Read::Missing if !host.machine.is_local() => {
                let machine = machine.clone();
                controls.push(
                    self.automation_button(
                        format!("automations-enable-{index}"),
                        "icons/plus.svg",
                        fl!("automations-enable"),
                    )
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.ask_to_enable_automations(&machine, false, window, cx);
                    }))
                    .into_any_element(),
                );
            }
            Read::Ready(status) => {
                if let Some(version) = self.automation_update(host) {
                    let machine = machine.clone();
                    controls.push(
                        self.automation_button(
                            format!("automations-update-{index}"),
                            "icons/refresh.svg",
                            fl!("automations-update-to", version = version.as_str()),
                        )
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.ask_to_enable_automations(&machine, true, window, cx);
                        }))
                        .into_any_element(),
                    );
                }
                if status.daemon.running {
                    controls.push(
                        self.automation_chip(
                            fl!("automations-daemon-up"),
                            theme.ink(theme.palette[2]),
                        )
                        .into_any_element(),
                    );
                } else {
                    controls.push(self.automation_action_button(
                        format!("automations-start-{index}"),
                        "icons/play.svg",
                        fl!("automations-start-daemon"),
                        host,
                        Action::Start,
                        cx,
                    ));
                }
                let (label, action) = if status.daemon.paused_all {
                    (fl!("automations-resume-all"), Action::ResumeAll)
                } else {
                    (fl!("automations-pause-all"), Action::PauseAll)
                };
                controls.push(self.automation_action_button(
                    format!("automations-pause-all-{index}"),
                    "icons/status-outline-unknown.svg",
                    label,
                    host,
                    action,
                    cx,
                ));
            }
            Read::Unread | Read::Missing | Read::Failed { .. } => {}
        }
        controls
    }

    fn automation_script_row(
        &self,
        host: &HostState,
        status: &Status,
        script: &ScriptStatus,
        now: i64,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let theme = &self.theme;
        let small = self.automation_small();
        let look = model::look(script);
        let opened = Opened::Script {
            machine: host.machine.clone(),
            path: script.path.clone(),
        };
        let last = script
            .last_run
            .as_ref()
            .map_or_else(|| "—".to_owned(), |run| model::result(run, now));
        let hover = theme.active;
        div()
            .id(SharedString::from(format!(
                "automations-script-{}-{}",
                host.label, script.path
            )))
            .px_3()
            .py(px(6.))
            .flex()
            .items_center()
            .gap_3()
            .border_t_1()
            .border_color(rgb(theme.active))
            .cursor_pointer()
            .hover(move |style| style.bg(rgb(hover)))
            .child(self.automation_icon(look, 14.))
            .child(
                div()
                    .w(px(220.))
                    .flex_none()
                    .truncate()
                    .child(script.name.clone()),
            )
            .child(
                div()
                    .w(px(110.))
                    .flex_none()
                    .truncate()
                    .text_size(small)
                    .text_color(rgb(theme.muted))
                    .child(model::schedule(script)),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_size(small)
                    .text_color(rgb(look.detail_color(theme)))
                    .child(model::detail(script, status, now)),
            )
            .child(
                div()
                    .w(px(110.))
                    .flex_none()
                    .truncate()
                    .text_size(small)
                    .text_color(rgb(theme.muted))
                    .child(last),
            )
            .child(
                div()
                    .w(px(44.))
                    .flex_none()
                    .flex()
                    .justify_end()
                    .text_size(small)
                    .child(model::tokens_today(script)),
            )
            .on_click(cx.listener(move |this, _, _, cx| {
                this.open_automation(opened.clone(), cx);
            }))
    }

    pub(super) fn automation_small(&self) -> Pixels {
        px(self.config.ui.size - 1.)
    }

    pub(super) fn automation_icon(&self, look: Look, size: f32) -> Svg {
        svg()
            .flex_none()
            .size(px(size))
            .path(look.icon())
            .text_color(rgb(look.color(&self.theme)))
    }

    pub(super) fn automation_button(
        &self,
        id: impl Into<SharedString>,
        icon: &'static str,
        label: impl Into<SharedString>,
    ) -> Stateful<Div> {
        let theme = &self.theme;
        let id = id.into();
        let selector = id.clone();
        div()
            .id(id)
            .debug_selector(move || selector.to_string())
            .flex_none()
            .px(px(10.))
            .h(px(24.))
            .flex()
            .items_center()
            .gap_1()
            .rounded(px(corners::CONTROL))
            .border_1()
            .border_color(rgb(theme.active))
            .cursor_pointer()
            .hover(|style| style.bg(rgb(theme.active)))
            .text_size(self.automation_small())
            .child(
                svg()
                    .path(icon)
                    .size(px(12.))
                    .text_color(rgb(theme.subtext())),
            )
            .child(label.into())
    }

    /// A button that asks the host to do `action`, saying so while it does.
    pub(super) fn automation_action_button(
        &self,
        id: impl Into<SharedString>,
        icon: &'static str,
        label: String,
        host: &HostState,
        action: Action,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let doing = host.doing(&action);
        let label = if doing { action.doing() } else { label };
        let machine = host.machine.clone();
        self.automation_button(id, icon, label)
            .when(host.busy.is_some() && !doing, |button| button.opacity(0.5))
            .on_click(cx.listener(move |this, _, _, cx| {
                this.automation_action(&machine, action.clone(), cx);
            }))
            .into_any_element()
    }

    pub(super) fn automation_chip(&self, text: impl Into<SharedString>, color: u32) -> Div {
        div()
            .flex_none()
            .px_2()
            .rounded(px(corners::SMALL))
            .bg(rgb(self.theme.active))
            .text_size(self.automation_small())
            .text_color(rgb(color))
            .child(text.into())
    }

    pub(super) fn automation_title(&self, text: impl Into<SharedString>) -> Div {
        div()
            .text_size(self.automation_small())
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(rgb(self.theme.subtext()))
            .child(text.into())
    }
}

/// The card's line after the machine's name: how it is reached, and its
/// daemon's version and uptime, or that it is down.
fn host_detail(host: &HostState) -> String {
    let mut parts: Vec<String> = host.machine.kind().map(str::to_owned).into_iter().collect();
    if let Some(status) = host.status() {
        let daemon = &status.daemon;
        let version = daemon.version.as_deref().unwrap_or(&status.version);
        if daemon.running {
            parts.push(fl!("automations-daemon-version", version = version));
            if let Some(started) = daemon.started_at {
                parts.push(fl!(
                    "automations-daemon-uptime",
                    duration = format::span(format::now() - started)
                ));
            }
        } else {
            parts.push(fl!(
                "automations-daemon-stopped",
                version = status.version.as_str()
            ));
        }
        if daemon.paused_all {
            parts.push(fl!("automations-all-paused"));
        }
    }
    parts.join(" · ")
}
