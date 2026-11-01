//! Drawing the Ports section from the latest listing: a header with the
//! count, the sort, and Refresh, then the user's servers and, apart, the
//! system's listeners, one row each (`row`).
// i18n: migrated

use super::{Listener, Report, SUPPORTED, Sort, order};
use crate::i18n::Localize;
use crate::{
    HerdrWindow, config::corners, direction::Logical, i18n::fl, listening_ports::Link, usage::Host,
};
use gpui::{prelude::*, *};
use herdr_control_protocol::protocol::hosts::PortAction;
use std::time::{Duration, SystemTime};

/// The Herdr workspace whose pane started a listener on this computer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Owned {
    pub endpoint: String,
    pub workspace: String,
    /// The workspace's name as the sidebar shows it.
    pub label: String,
}

/// `up 5m`, `up 2h 3m`, `up 4d 1h`: the two largest units.
pub(super) fn uptime_text(uptime: Duration) -> SharedString {
    let seconds = uptime.as_secs();
    let (days, hours, minutes) = (seconds / 86_400, seconds / 3600 % 24, seconds / 60 % 60);
    match (days, hours) {
        (0, 0) => fl!("local-ports-up-minutes", minutes = minutes),
        (0, _) => fl!("local-ports-up-hours", hours = hours, minutes = minutes),
        _ => fl!("local-ports-up-days", days = days, hours = hours),
    }
    .into()
}

fn report_text(report: Report) -> Option<SharedString> {
    let done = match report.action? {
        PortAction::Stop => fl!("local-ports-report-stop", count = report.done),
        PortAction::Pause => fl!("local-ports-report-pause", count = report.done),
        PortAction::Resume => fl!("local-ports-report-resume", count = report.done),
    };
    let mut text = done.to_string();
    for extra in [
        (report.killed > 0).then(|| fl!("local-ports-report-killed", count = report.killed)),
        (report.gone > 0).then(|| fl!("local-ports-report-gone", count = report.gone)),
        (report.failed > 0).then(|| fl!("local-ports-report-failed", count = report.failed)),
    ]
    .into_iter()
    .flatten()
    {
        text.push(' ');
        text.push_str(&extra);
    }
    Some(text.into())
}

impl HerdrWindow {
    /// The workspace of a daemon on this computer that started the server
    /// on `number`, as the workspace chips found it.
    pub(crate) fn local_port_owner(&self, number: u16) -> Option<Owned> {
        self.endpoints
            .iter()
            .enumerate()
            .filter(|(_, endpoint)| {
                endpoint.enabled && Host::of(&endpoint.connection.target) == Some(Host::Local)
            })
            .find_map(|(index, endpoint)| {
                let daemon = crate::listening_ports::Daemon::of(&endpoint.connection.target)?;
                let workspace = self.listening_ports.workspace_of(&daemon, number)?;
                let live = if index == self.selected_endpoint {
                    &self.live
                } else {
                    &endpoint.live
                };
                let label = live
                    .snapshot
                    .as_ref()
                    .and_then(|snapshot| {
                        snapshot
                            .workspaces
                            .iter()
                            .find(|known| known.workspace_id == workspace)
                    })
                    .map_or_else(|| workspace.to_owned(), |known| known.label.clone());
                Some(Owned {
                    endpoint: endpoint.id.clone(),
                    workspace: workspace.to_owned(),
                    label,
                })
            })
    }

    /// Opens a listener's page: in a browser tab of the workspace that
    /// started it, else in the system's browser.
    pub(super) fn open_listener_page(
        &mut self,
        listener: &Listener,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(url) = listener.url() else {
            return;
        };
        match self.local_port_owner(listener.number) {
            Some(owned) => {
                self.open_port_link(
                    &owned.endpoint,
                    &owned.workspace,
                    &Link::Page(url),
                    window,
                    cx,
                );
            }
            None => cx.open_url(url.as_str()),
        }
    }

    pub(super) fn resume_local_port(&mut self, listener: &Listener, cx: &mut Context<Self>) {
        if let Err(error) = self
            .local_ports
            .signal(PortAction::Resume, vec![listener.identity])
        {
            self.local_error = Some(error.localize());
        }
        cx.notify();
    }

    pub(crate) fn render_local_ports_section(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = &self.theme;
        let font = &self.config.ui;
        let mut body = div().p_5().flex().flex_col().gap_2();
        let note = |text: SharedString, selector: &'static str| {
            div()
                .debug_selector(move || selector.into())
                .text_color(rgb(theme.muted))
                .child(text)
        };
        let listing = self.local_ports.listing();
        if !SUPPORTED {
            body = body.child(note(
                fl!("local-ports-unsupported").into(),
                "local-ports-unsupported",
            ));
        } else if let Some(error) = self.local_ports.error() {
            body = body.child(
                div()
                    .debug_selector(|| "local-ports-error".into())
                    .text_color(rgb(theme.ink(theme.palette[3])))
                    .child(fl!("local-ports-failed", error = error)),
            );
        }
        if let Some(text) = self.local_ports.report().and_then(report_text) {
            body = body.child(note(text, "local-ports-report"));
        }
        match listing {
            None if SUPPORTED && self.local_ports.error().is_none() => {
                body = body.child(note(
                    fl!("local-ports-reading").into(),
                    "local-ports-reading",
                ));
            }
            Some([]) => {
                body = body.child(note(fl!("local-ports-empty").into(), "local-ports-empty"));
            }
            Some(listeners) => {
                let now = SystemTime::now();
                let home = std::env::var_os("HOME").map(std::path::PathBuf::from);
                let mut system_shown = false;
                for (index, listener) in order(listeners, self.local_ports.sort)
                    .into_iter()
                    .enumerate()
                {
                    if listener.system && !system_shown {
                        system_shown = true;
                        body = body.child(
                            div()
                                .debug_selector(|| "local-ports-system".into())
                                .pt_3()
                                .text_size(px(font.size - 1.))
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(rgb(theme.muted))
                                .child(fl!("local-ports-system")),
                        );
                    }
                    body =
                        body.child(self.local_port_row(index, listener, now, home.as_deref(), cx));
                }
            }
            None => {}
        }
        div()
            .id("local-ports-section")
            .debug_selector(|| "local-ports-section".into())
            .flex()
            .flex_col()
            .flex_1()
            .size_full()
            .min_w_0()
            .min_h_0()
            .bg(rgb(theme.background))
            .text_color(rgb(theme.foreground))
            .text_size(px(font.size))
            .child(self.local_ports_header(listing.map(<[Listener]>::len), cx))
            .child(
                div()
                    .id("local-ports-scroll")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .child(body),
            )
            .into_any_element()
    }

    fn local_ports_header(&self, count: Option<usize>, cx: &mut Context<Self>) -> Div {
        let theme = &self.theme;
        let font = &self.config.ui;
        div()
            .debug_selector(|| "local-ports-header".into())
            .flex_none()
            .px_5()
            .py_3()
            .row()
            .flex_wrap()
            .items_center()
            .gap_3()
            .border_b_1()
            .border_color(rgb(theme.active))
            .child(
                div()
                    .text_size(px(font.size + 3.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(fl!("local-ports-title")),
            )
            .children(count.map(|count| {
                div()
                    .text_color(rgb(theme.muted))
                    .child(fl!("local-ports-count", count = count))
            }))
            .child(div().flex_1())
            .child(self.local_ports_sort(cx))
            .when(SUPPORTED, |header| {
                header.child(
                    self.local_ports_button(
                        "local-ports-refresh".into(),
                        fl!("local-ports-refresh").into(),
                    )
                    .child(
                        svg()
                            .path("icons/refresh.svg")
                            .size(px(12.))
                            .text_color(rgb(theme.subtext())),
                    )
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.local_ports.refresh();
                        cx.notify();
                    })),
                )
            })
    }

    /// Port, CPU, and Memory: how the rows are ordered.
    fn local_ports_sort(&self, cx: &mut Context<Self>) -> Div {
        let theme = &self.theme;
        let small = px(self.config.ui.size - 1.);
        let mut toggles = div()
            .flex_none()
            .row()
            .items_center()
            .p(px(2.))
            .gap(px(2.))
            .rounded(px(corners::CONTROL))
            .bg(rgb(theme.surface));
        for sort in Sort::ALL {
            let (key, label) = match sort {
                Sort::Port => ("port", fl!("local-ports-sort-port")),
                Sort::Cpu => ("cpu", fl!("local-ports-sort-cpu")),
                Sort::Memory => ("memory", fl!("local-ports-sort-memory")),
            };
            let on = self.local_ports.sort == sort;
            let selector = format!("local-ports-sort-{key}");
            toggles = toggles.child(
                div()
                    .id(SharedString::from(selector.clone()))
                    .debug_selector(move || selector.clone())
                    .px(px(8.))
                    .h(px(20.))
                    .row()
                    .items_center()
                    .rounded(px(corners::SMALL))
                    .text_size(small)
                    .whitespace_nowrap()
                    .cursor_pointer()
                    .when(on, |toggle| {
                        toggle
                            .bg(rgb(theme.background))
                            .font_weight(FontWeight::SEMIBOLD)
                    })
                    .when(!on, |toggle| toggle.text_color(rgb(theme.subtext())))
                    .child(label)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.local_ports.sort = sort;
                        cx.notify();
                    })),
            );
        }
        toggles
    }

    pub(super) fn local_ports_button(
        &self,
        id: SharedString,
        label: SharedString,
    ) -> Stateful<Div> {
        let theme = &self.theme;
        let selector = id.to_string();
        div()
            .id(id)
            .debug_selector(move || selector.clone())
            .flex_none()
            .px(px(10.))
            .h(px(24.))
            .row()
            .items_center()
            .gap_1()
            .rounded(px(corners::CONTROL))
            .border_1()
            .border_color(rgb(theme.active))
            .cursor_pointer()
            .hover(|style| style.bg(rgb(theme.active)))
            .text_size(px(self.config.ui.size - 1.))
            .whitespace_nowrap()
            .child(label)
    }
}
