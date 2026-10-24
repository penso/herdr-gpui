//! The Overview's host column: every enabled host with its agents, CPU and
//! memory, and listening ports. Each host folds to one line, and the whole
//! column folds to a strip of load dots.

use crate::{
    HerdrWindow,
    devices_overview::{Device, Link, Look, Tally},
    system_load::{self, CPU_WARN, GPU_WARN, MEMORY_WARN, VRAM_WARN},
};
use gpui::{prelude::*, *};

const WIDTH: f32 = 290.;
const STRIP: f32 = 40.;
const METER: f32 = 150.;
/// Ports a host card lists; the count says the rest.
const PORTS: usize = 6;

impl HerdrWindow {
    pub(super) fn render_overview_hosts(&self, look: &Look, cx: &mut Context<Self>) -> AnyElement {
        let devices: Vec<Device> = self
            .overview_devices()
            .into_iter()
            .filter(|device| device.link != Link::Disabled)
            .collect();
        let theme = look.theme;
        let toggle = div()
            .id("overview-hosts-toggle")
            .debug_selector(|| "overview-hosts-toggle".into())
            .size(px(24.))
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(crate::config::corners::CONTROL))
            .cursor_pointer()
            .hover(|style| style.bg(rgb(theme.active)))
            .child(
                svg()
                    .path("icons/panel-right.svg")
                    .size(px(14.))
                    .text_color(rgb(theme.muted)),
            )
            .on_click(cx.listener(|this, _, _, cx| {
                this.overview.hosts_closed = Some(!this.overview_hosts_closed());
                cx.notify();
            }));
        if self.overview_hosts_closed() {
            return div()
                .debug_selector(|| "overview-hosts-strip".into())
                .w(px(STRIP))
                .flex_none()
                .pt_3()
                .flex()
                .flex_col()
                .items_center()
                .gap_3()
                .border_l_1()
                .border_color(rgb(theme.active))
                .child(toggle)
                .children(devices.iter().map(|device| {
                    let cpu = self.host_cpu(device);
                    div()
                        .flex()
                        .flex_col()
                        .items_center()
                        .gap_1()
                        .child(
                            svg()
                                .path("icons/computer.svg")
                                .size(px(14.))
                                .text_color(rgb(theme.subtext())),
                        )
                        .child(self.load_dot(look, cpu))
                        .child(
                            div().text_size(px(9.)).text_color(rgb(theme.muted)).child(
                                cpu.map_or_else(|| "—".to_owned(), |cpu| format!("{cpu:.0}%")),
                            ),
                        )
                }))
                .into_any_element();
        }
        div()
            .id("overview-hosts")
            .debug_selector(|| "overview-hosts".into())
            .w(px(WIDTH))
            .flex_none()
            .h_full()
            .overflow_y_scroll()
            .p_3()
            .flex()
            .flex_col()
            .gap_2()
            .border_l_1()
            .border_color(rgb(theme.active))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .text_size(look.small())
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(rgb(theme.subtext()))
                    .child("Hosts")
                    .child(
                        div()
                            .text_color(rgb(theme.muted))
                            .child(devices.len().to_string()),
                    )
                    .child(div().flex_1())
                    .child(toggle),
            )
            .children(
                devices
                    .iter()
                    .map(|device| self.host_card(look, device, cx)),
            )
            .into_any_element()
    }

    fn host_cpu(&self, device: &Device) -> Option<f32> {
        let host = device.host.as_ref()?;
        self.system_load.get(host)?.latest()?.cpu
    }

    fn load_dot(&self, look: &Look, cpu: Option<f32>) -> Div {
        let theme = look.theme;
        let color = cpu.map_or(theme.active, |cpu| {
            system_load::severity(cpu, CPU_WARN, theme, theme.ink(theme.palette[2]))
        });
        div().flex_none().size(px(7.)).rounded_full().bg(rgb(color))
    }

    fn host_card(&self, look: &Look, device: &Device, cx: &mut Context<Self>) -> Div {
        let theme = look.theme;
        let folded = self.overview.folded.contains(&device.id);
        let sample = device
            .host
            .as_ref()
            .and_then(|host| self.system_load.get(host))
            .and_then(system_load::Reading::latest);
        let cpu = sample.and_then(|sample| sample.cpu);
        let memory = sample.and_then(|sample| sample.memory.map(|memory| memory.percent()));
        // Only hosts with a GPU get its meters, while GPUs are read.
        let gpu = sample
            .and_then(|sample| sample.gpu)
            .filter(|_| self.config.gpu_sampled());
        let id = device.id.clone();
        let agents = match device.link {
            Link::Online => agents_line(device.tally),
            Link::Connecting => "Connecting…".to_owned(),
            Link::Offline | Link::Disabled => "Not connected".to_owned(),
        };
        let header = div()
            .id(SharedString::from(format!("overview-host-{}", device.id)))
            .debug_selector({
                let id = format!("overview-host-{}", device.id);
                move || id.clone()
            })
            .p_2()
            .flex()
            .items_center()
            .gap_2()
            .rounded(px(crate::config::corners::PANEL))
            .cursor_pointer()
            .hover(|style| style.bg(rgb(theme.active)))
            .child(
                svg()
                    .path(if folded {
                        "icons/chevron-right.svg"
                    } else {
                        "icons/chevron-down.svg"
                    })
                    .size(px(10.))
                    .flex_none()
                    .text_color(rgb(theme.muted)),
            )
            .child(look.link(device.link))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(device.label.clone()),
            )
            .when(folded, |header| {
                let total = device.tally.total();
                header
                    .when(total > 0, |header| {
                        header.child(look.mono(total.to_string()))
                    })
                    .child(self.load_dot(look, cpu))
                    .child(
                        look.mono(cpu.map_or_else(|| "—".to_owned(), |cpu| format!("{cpu:.0}%"))),
                    )
            })
            .on_click(cx.listener(move |this, _, _, cx| {
                if !this.overview.folded.remove(&id) {
                    this.overview.folded.insert(id.clone());
                }
                cx.notify();
            }));
        let ports = self.host_ports(device);
        let card = format!("overview-host-card-{}", device.id);
        div()
            .debug_selector(move || card.clone())
            .flex()
            .flex_col()
            .rounded(px(crate::config::corners::PANEL))
            .bg(rgb(theme.surface))
            .child(header)
            .when(!folded, |card| {
                card.child(
                    div()
                        .px_2()
                        .pb_2()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .child(
                            div()
                                .text_size(look.small())
                                .text_color(rgb(theme.muted))
                                .child(agents),
                        )
                        .child(meter(look, "CPU", cpu, CPU_WARN))
                        .child(meter(look, "MEM", memory, MEMORY_WARN))
                        .when_some(gpu, |card, gpu| {
                            card.when(gpu.busy.is_some(), |card| {
                                card.child(meter(look, "GPU", gpu.busy, GPU_WARN))
                            })
                            .when_some(gpu.memory, |card, memory| {
                                card.child(meter(look, "VRAM", Some(memory.percent()), VRAM_WARN))
                            })
                        })
                        .when(!ports.is_empty(), |card| {
                            let more = ports.len().saturating_sub(PORTS);
                            card.child(
                                div()
                                    .pt_1()
                                    .flex()
                                    .flex_wrap()
                                    .gap_1()
                                    .children(ports.into_iter().take(PORTS).map(|port| {
                                        div()
                                            .px(px(5.))
                                            .rounded(px(crate::config::corners::SMALL))
                                            .border_1()
                                            .border_color(rgb(theme.active))
                                            .child(look.mono(port))
                                    }))
                                    .when(more > 0, |ports| {
                                        ports.child(look.mono(format!("+{more}")))
                                    }),
                            )
                        }),
                )
            })
    }

    /// `:3000 vite` for each port the host's workspaces listen on, lowest
    /// first, when ports are scanned.
    fn host_ports(&self, device: &Device) -> Vec<String> {
        let Some(endpoint) = self.endpoints.get(device.index) else {
            return Vec::new();
        };
        let live = if device.index == self.selected_endpoint {
            &self.live
        } else {
            &endpoint.live
        };
        let (Some(daemon), Some(snapshot)) = (
            crate::listening_ports::Daemon::of(&endpoint.connection.target),
            live.snapshot.as_deref(),
        ) else {
            return Vec::new();
        };
        let mut ports: Vec<(u16, String)> = snapshot
            .workspaces
            .iter()
            .filter_map(|workspace| self.listening_ports.get(&daemon, &workspace.workspace_id))
            .flat_map(|listed| listed.ports.iter())
            .map(|port| (port.number, port.process.clone()))
            .collect();
        ports.sort();
        ports.dedup();
        ports
            .into_iter()
            .map(|(number, process)| format!(":{number} {process}"))
            .collect()
    }
}

/// An online host's agents, leaving out counts of none as the rest of the
/// Overview does.
pub(super) fn agents_line(tally: Tally) -> String {
    let agents = match tally.total() {
        0 => return "No agents".to_owned(),
        1 => "1 agent".to_owned(),
        total => format!("{total} agents"),
    };
    match tally.working {
        0 => agents,
        working => format!("{agents} · {working} working"),
    }
}

fn meter(look: &Look, label: &'static str, percent: Option<f32>, warn: f32) -> Div {
    let theme = look.theme;
    let fill = percent
        .map(|percent| system_load::severity(percent, warn, theme, theme.ink(theme.palette[2])));
    div()
        .flex()
        .items_center()
        .gap_2()
        .child(div().w(px(30.)).child(look.mono(label)))
        .child(
            div()
                .w(px(METER))
                .h(px(4.))
                .rounded_full()
                .bg(rgb(theme.active))
                .when_some(percent.zip(fill), |bar, (percent, fill)| {
                    bar.child(
                        div()
                            .h_full()
                            .rounded_full()
                            .w(px(METER * percent.clamp(0., 100.) / 100.))
                            .bg(rgb(fill)),
                    )
                }),
        )
        .child(
            look.mono(percent.map_or_else(|| "—".to_owned(), |percent| format!("{percent:.0}%"))),
        )
}
