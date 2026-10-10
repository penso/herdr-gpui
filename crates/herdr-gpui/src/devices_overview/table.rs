//! The device table: one row per device with its agents, CPU, memory, home
//! disk and uptime. Clicking a row opens it on a list of its agents, and
//! clicking an agent shows its pane.

use super::{Device, Link, view::Look};
use crate::{
    HerdrWindow,
    browser::{Slot, TabId},
    system_load::{self, CPU_WARN, DISK_WARN, MEMORY_WARN, SystemLoad},
};
use gpui::{prelude::*, *};
use herdr_client::protocol::AgentStatus;
use std::collections::HashSet;

const DEVICE_MIN: f32 = 180.;
/// Every column but the device's, which takes the rest.
const COLUMNS: [(&str, f32); 6] = [
    ("Agents", 230.),
    ("CPU", 84.),
    ("Memory", 84.),
    ("Disk free", 96.),
    ("Uptime", 76.),
    ("", 12.),
];
const METER_WIDTH: f32 = 64.;
/// Where an open row's agents start: under the device's name, past its dot.
const AGENT_INDENT: f32 = 31.;

/// A row's cells: the device, its agents, then the load columns as one
/// group, so a narrow tab wraps that group onto a line of its own, then wraps
/// its columns if needed. The header wraps in step with every row.
fn cells(
    device: impl IntoElement,
    agents: impl IntoElement,
    load: [AnyElement; 5],
    selector: String,
) -> Div {
    div()
        .flex()
        .flex_wrap()
        .items_center()
        .gap_x_3()
        .gap_y_2()
        .child(div().flex_1().min_w(px(DEVICE_MIN)).child(device))
        .child(div().flex_none().w(px(COLUMNS[0].1)).child(agents))
        .child(
            div()
                .debug_selector({
                    let selector = selector.clone();
                    move || selector
                })
                .flex()
                .flex_none()
                .max_w_full()
                .flex_wrap()
                .items_center()
                .gap_3()
                .children(load.into_iter().zip(&COLUMNS[1..]).enumerate().map(
                    |(index, (cell, (_, width)))| {
                        let selector = format!("{selector}-{index}");
                        div()
                            .debug_selector(move || selector)
                            .flex_none()
                            .w(px(*width))
                            .child(cell)
                    },
                )),
        )
}

#[allow(clippy::too_many_arguments)]
pub(super) fn devices(
    look: &Look,
    shown: &[&Device],
    load: &SystemLoad,
    slot: Slot,
    tab: TabId,
    expanded: &HashSet<String>,
    cx: &mut Context<HerdrWindow>,
) -> Div {
    let theme = look.theme;
    let label = |index: usize| COLUMNS[index].0.into_any_element();
    let header = cells(
        "Device",
        COLUMNS[0].0,
        [label(1), label(2), label(3), label(4), label(5)],
        slot.selector("devices-load-header"),
    )
    .px_3()
    .py_2()
    .border_b_1()
    .border_color(rgb(theme.active))
    .text_size(look.small())
    .text_color(rgb(theme.subtext()));
    let rows = shown.iter().enumerate().map(|(position, device)| {
        let reading = device
            .host
            .as_ref()
            .filter(|_| device.link == Link::Online)
            .and_then(|host| load.get(host));
        let sample = reading.and_then(system_load::Reading::latest);
        let disk = sample.and_then(|sample| sample.disk);
        let open = expanded.contains(&device.id);
        let id = device.id.clone();
        let row_id = device.id.clone();
        let row = cells(
            name(look, device),
            agents(look, device),
            [
                meter(look, None, sample.and_then(|sample| sample.cpu), CPU_WARN)
                    .into_any_element(),
                meter(
                    look,
                    None,
                    sample.and_then(|sample| sample.memory.map(|memory| memory.percent())),
                    MEMORY_WARN,
                )
                .into_any_element(),
                meter(
                    look,
                    disk.map(|disk| system_load::storage(disk.available)),
                    disk.map(|disk| disk.used_percent()),
                    DISK_WARN,
                )
                .into_any_element(),
                look.mono(
                    sample
                        .and_then(|sample| sample.uptime)
                        .map_or_else(|| "—".to_owned(), system_load::uptime),
                )
                .into_any_element(),
                svg()
                    .size(px(COLUMNS[5].1))
                    .path(if open {
                        "icons/chevron-down.svg"
                    } else {
                        "icons/chevron-right.svg"
                    })
                    .text_color(rgb(theme.muted))
                    .into_any_element(),
            ],
            slot.selector(&format!("devices-load-{}", device.id)),
        )
        .id(SharedString::from(
            slot.selector(&format!("devices-row-{}", device.id)),
        ))
        .debug_selector(move || slot.selector(&format!("devices-row-{row_id}")))
        .px_3()
        .py_2()
        .cursor_pointer()
        .hover(|style| style.bg(rgb(theme.active)))
        .on_click(cx.listener(move |this, _, _, cx| {
            if let Some(page) = this.devices_overview.pages.get_mut(&tab) {
                if !page.expanded.remove(&id) {
                    page.expanded.insert(id.clone());
                }
                cx.notify();
            }
        }));
        div()
            .flex()
            .flex_col()
            .when(position > 0, |unit| {
                unit.border_t_1().border_color(rgb(theme.active))
            })
            .child(row)
            .when(open, |unit| unit.child(agent_list(look, device, slot, cx)))
    });
    look.panel()
        .debug_selector(move || slot.selector("devices-table"))
        .bg(rgb(theme.background))
        .flex()
        .flex_col()
        .child(header)
        .children(rows)
        .when(shown.is_empty(), |table| {
            table.child(
                div()
                    .p_4()
                    .text_color(rgb(theme.muted))
                    .child("No device, agent or workspace matches the search."),
            )
        })
}

/// An open row's agents, working first; clicking one shows its pane.
fn agent_list(look: &Look, device: &Device, slot: Slot, cx: &mut Context<HerdrWindow>) -> Div {
    let theme = look.theme;
    let list = div()
        .debug_selector({
            let id = device.id.clone();
            move || slot.selector(&format!("devices-agents-{id}"))
        })
        .flex()
        .flex_col()
        .pb_2()
        .bg(rgb(theme.surface));
    if device.agents.is_empty() {
        return list.child(
            div()
                .pl(px(AGENT_INDENT))
                .pt_2()
                .text_color(rgb(theme.muted))
                .child(match device.link {
                    Link::Online => "No agents on this device.",
                    _ => "Not connected, so its agents are unknown.",
                }),
        );
    }
    list.children(device.agents.iter().map(|agent| {
        let (endpoint, pane) = (device.id.clone(), agent.pane_id.clone());
        let (tab, workspace) = (agent.tab_id.clone(), agent.workspace_id.clone());
        div()
            .id(SharedString::from(slot.selector(&format!(
                "devices-agent-{}-{}",
                device.id, agent.pane_id
            ))))
            .debug_selector({
                let id = format!("devices-agent-{}-{}", device.id, agent.pane_id);
                move || slot.selector(&id)
            })
            .flex()
            .items_center()
            .gap_2()
            .pl(px(AGENT_INDENT))
            .pr_3()
            .py_1()
            .cursor_pointer()
            .hover(|style| style.bg(rgb(theme.active)))
            .child(look.dot(agent.status))
            .child(
                svg()
                    .flex_none()
                    .size(px(13.))
                    .path(crate::icons::AgentIcon::from_identity(agent.identity.as_deref()).path())
                    .text_color(rgb(theme.foreground)),
            )
            .child(
                div()
                    .flex_none()
                    .w(px(120.))
                    .truncate()
                    .child(agent.name.clone()),
            )
            .child(
                look.mono(agent.workspace.clone())
                    .flex_1()
                    .min_w_0()
                    .truncate(),
            )
            .child(
                div()
                    .flex_none()
                    .text_size(look.small())
                    .text_color(rgb(look.status(agent.status)))
                    .child(crate::sidebar::status_text(agent.status)),
            )
            .on_click(cx.listener(move |this, _, window, cx| {
                // Read before navigating, which may switch devices.
                let on_screen = this.endpoints[this.selected_endpoint].id == endpoint
                    && this
                        .browser_key()
                        .is_some_and(|(_, shown)| shown == workspace);
                this.navigate_endpoint(&endpoint, crate::NavigationTarget::Pane(&pane), cx);
                // The window leaves a page for its terminal when the daemon's
                // focus moves to another tab or workspace. An agent in the
                // tab already focused moves nothing, so show it here.
                if on_screen {
                    this.terminal_focus_moved(&tab, false, cx);
                }
                window.focus(&this.focus, cx);
            }))
    }))
}

fn name(look: &Look, device: &Device) -> Div {
    div()
        .flex()
        .items_center()
        .gap_2()
        .min_w_0()
        .child(look.link(device.link))
        .child(
            div()
                .flex()
                .flex_col()
                .min_w_0()
                .child(
                    div()
                        .truncate()
                        .font_weight(FontWeight::MEDIUM)
                        .child(device.label.clone()),
                )
                .child(look.mono(device.address.clone()).truncate()),
        )
}

/// A dot per agent, then `2/5 working`, then how many wait for input.
fn agents(look: &Look, device: &Device) -> Div {
    let theme = look.theme;
    let tally = device.tally;
    let text = match device.link {
        Link::Offline => "Offline".to_owned(),
        Link::Connecting => "Connecting".to_owned(),
        Link::Disabled => "Disabled".to_owned(),
        Link::Online if tally.total() == 0 => "No agents".to_owned(),
        Link::Online => format!("{}/{} working", tally.working, tally.total()),
    };
    let blocked = look.status(AgentStatus::Blocked);
    div()
        .flex()
        .items_center()
        .gap_2()
        .min_w_0()
        .child(
            div()
                .flex()
                .flex_none()
                .items_center()
                .gap(px(3.))
                .children(device.dots.iter().map(|status| look.dot(*status))),
        )
        .child(look.mono(text).min_w_0().truncate())
        .when(tally.blocked > 0, |row| {
            row.child(
                div()
                    .flex_none()
                    .px(px(5.))
                    .rounded(px(crate::config::corners::SMALL))
                    .bg(rgb(crate::config::mix(theme.background, blocked, 18)))
                    .text_color(rgb(blocked))
                    .text_size(look.small())
                    .child(format!("{} blocked", tally.blocked)),
            )
        })
}

/// A share over a thin bar, or a dash before the first sample.
fn meter(look: &Look, label: Option<String>, percent: Option<f32>, warn: f32) -> Div {
    let theme = look.theme;
    let Some(percent) = percent else {
        return look.mono("—");
    };
    let fill = system_load::severity(
        percent,
        warn,
        theme,
        crate::config::mix(theme.background, theme.foreground, 70),
    );
    div()
        .flex()
        .flex_col()
        .gap(px(3.))
        .child(
            look.mono(label.unwrap_or_else(|| format!("{percent:.0}%")))
                .text_color(rgb(theme.foreground)),
        )
        .child(
            div()
                .w(px(METER_WIDTH))
                .h(px(3.))
                .rounded_full()
                .bg(rgb(theme.active))
                .child(
                    div()
                        .h_full()
                        .rounded_full()
                        .w(px(METER_WIDTH * percent.clamp(0., 100.) / 100.))
                        .bg(rgb(fill)),
                ),
        )
}
