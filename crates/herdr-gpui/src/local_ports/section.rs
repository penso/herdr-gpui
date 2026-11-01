//! Drawing the Ports section from the latest listing: a header with the
//! count and Refresh, then one row per listening port with its process, its
//! working directory, how long it has run, the Herdr workspace that started
//! it when one did, and Open, Pause or Resume, and Stop.
// i18n: migrated

use super::{Listener, Pending, Report, SUPPORTED, home_relative};
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
fn uptime_text(uptime: Duration) -> SharedString {
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
    fn open_listener_page(
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

    fn resume_local_port(&mut self, listener: &Listener, cx: &mut Context<Self>) {
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
                for (index, listener) in listeners.iter().enumerate() {
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

    fn local_ports_button(&self, id: SharedString, label: SharedString) -> Stateful<Div> {
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

    fn local_port_row(
        &self,
        index: usize,
        listener: &Listener,
        now: SystemTime,
        home: Option<&std::path::Path>,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let theme = &self.theme;
        let small = px(self.config.ui.size - 1.);
        let acting = self.local_ports.acting();
        let owner = self.local_port_owner(listener.number);
        let directory = listener.directory.as_deref().map(|path| {
            let project = path
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| home_relative(path, home));
            (project, home_relative(path, home))
        });
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
        let action = |id: &str, label: SharedString| {
            self.local_ports_button(format!("local-port-{index}-{id}").into(), label)
        };
        let mut actions = div().flex_none().row().items_center().gap_1();
        if listener.url().is_some() {
            let opened = listener.clone();
            actions = actions.child(action("open", fl!("local-ports-open").into()).on_click(
                cx.listener(move |this, _, window, cx| {
                    this.open_listener_page(&opened, window, cx)
                }),
            ));
        }
        if !acting {
            let chosen = listener.clone();
            actions = actions.child(if listener.paused {
                action("resume", fl!("local-ports-resume").into()).on_click(
                    cx.listener(move |this, _, _, cx| this.resume_local_port(&chosen, cx)),
                )
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
            actions = actions.child(
                action("stop", fl!("local-ports-stop").into())
                    .text_color(crate::menu::danger(theme))
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
            );
        }
        let selector = format!("local-port-{index}");
        div()
            .id(("local-port", index))
            .debug_selector(move || selector.clone())
            .px_3()
            .py_2()
            .row()
            .items_center()
            .gap_3()
            .rounded(px(corners::PANEL))
            .bg(rgb(theme.surface))
            .child(
                div()
                    .flex_none()
                    .w(px(120.))
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
                            .child(div().min_w_0().truncate().child(listener.process.clone()))
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
                            .children(directory.map(|(project, path)| {
                                div().min_w_0().truncate().child(fl!(
                                    "local-ports-directory",
                                    project = project.as_str(),
                                    path = path.as_str()
                                ))
                            }))
                            .children(
                                listener
                                    .uptime(now)
                                    .map(|uptime| div().flex_none().child(uptime_text(uptime))),
                            ),
                    ),
            )
            .child(actions)
    }
}
