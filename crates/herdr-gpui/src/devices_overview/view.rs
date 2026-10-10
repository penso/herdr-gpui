//! Drawing an overview tab: its header and search, the activity chart, the
//! totals, and the device table, all from state the tick prepared.

use super::{Device, Link, Tally, chart, table};
use crate::{
    HerdrWindow,
    browser::{Slot, Tab},
    config::{Theme, corners},
    fonts::StyledFont,
    sidebar::Indicators,
};
use gpui::{prelude::*, *};
use herdr_client::protocol::AgentStatus;

/// What every part of the page, and the device picker's activity card,
/// draws with.
pub(crate) struct Look<'a> {
    pub theme: &'a Theme,
    pub ui: &'a crate::config::FontConfig,
    pub mono: &'a crate::config::FontConfig,
    pub indicators: Indicators,
}

impl Look<'_> {
    pub fn small(&self) -> Pixels {
        px(self.ui.size - 1.)
    }

    pub fn status(&self, status: AgentStatus) -> u32 {
        self.indicators.color(status)
    }

    /// Numbers, sizes and addresses, in the terminal face.
    pub fn mono(&self, text: impl Into<SharedString>) -> Div {
        div()
            .text_font(self.mono)
            .text_size(px(self.mono.size - 1.))
            .text_color(rgb(self.theme.muted))
            .whitespace_nowrap()
            .child(text.into())
    }

    /// A status dot as the sidebar draws it: idle hollow, the rest filled.
    pub fn dot(&self, status: AgentStatus) -> Div {
        let color = self.status(status);
        let dot = div().flex_none().size(px(7.)).rounded_full();
        if status == AgentStatus::Idle {
            dot.border_1().border_color(rgb(color))
        } else {
            dot.bg(rgb(color))
        }
    }

    pub fn link(&self, link: Link) -> Div {
        div()
            .flex_none()
            .size(px(7.))
            .rounded_full()
            .bg(rgb(match link {
                Link::Online => crate::menu::online(self.theme),
                Link::Connecting => self.status(AgentStatus::Working),
                Link::Offline | Link::Disabled => self.theme.muted,
            }))
    }

    pub fn panel(&self) -> Div {
        div()
            .flex_none()
            .rounded(px(corners::PANEL))
            .border_1()
            .border_color(rgb(self.theme.active))
            .bg(rgb(self.theme.surface))
            .overflow_hidden()
    }

    pub fn title(&self, title: &'static str, count: usize) -> Div {
        div()
            .flex()
            .items_center()
            .gap_2()
            .child(div().font_weight(FontWeight::SEMIBOLD).child(title))
            .child(self.mono(count.to_string()))
    }

    pub fn legend(&self) -> Div {
        let item = |status: AgentStatus, label: &'static str| {
            div()
                .flex()
                .items_center()
                .gap_1()
                .child(self.dot(status))
                .child(label)
        };
        div()
            .flex()
            .flex_wrap()
            .gap_3()
            .text_size(self.small())
            .text_color(rgb(self.theme.subtext()))
            .child(item(AgentStatus::Working, "Working"))
            .child(item(AgentStatus::Blocked, "Blocked"))
            .child(item(AgentStatus::Done, "Done"))
            .child(item(AgentStatus::Idle, "Idle"))
    }
}

impl HerdrWindow {
    /// The theme, faces, and the sidebar's own status colours.
    pub(crate) fn overview_look(&self, cx: &App) -> Look<'_> {
        Look {
            theme: &self.theme,
            ui: &self.config.ui,
            mono: &self.config.terminal,
            indicators: Indicators::new(
                self.settings.shared.as_ref(),
                matches!(
                    cx.window_appearance(),
                    WindowAppearance::Light | WindowAppearance::VibrantLight
                ),
                &self.theme,
            ),
        }
    }

    /// An overview tab, drawn in `slot` where a page would be.
    pub(crate) fn render_devices_tab(
        &self,
        slot: Slot,
        tab: &Tab,
        gap: f32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let id = tab.id;
        let Some(page) = self.devices_overview.pages.get(&id) else {
            return div()
                .id(SharedString::from(slot.selector("devices-loading")))
                .size_full()
                .pl(px(gap))
                .into_any_element();
        };
        let look = self.overview_look(cx);
        let devices = self.overview_devices();
        let query = page.search.read(cx).text().to_owned();
        let shown: Vec<&Device> = devices
            .iter()
            .filter(|device| device.matches(&query))
            .collect();
        let focus = page.focus.clone();
        let search_focus = page.search.read(cx).focus.clone();
        div()
            .id(SharedString::from(slot.selector("devices-tab")))
            .debug_selector(move || slot.selector("devices-tab"))
            .size_full()
            .min_w_0()
            .pl(px(gap))
            .track_focus(&focus)
            .bg(rgb(look.theme.background))
            .text_color(rgb(look.theme.foreground))
            .text_font(look.ui)
            .text_size(px(look.ui.size))
            // The page holds the keyboard while used, so typing never reaches
            // a terminal in another group.
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |_, _, window, cx| {
                    if !search_focus.is_focused(window) {
                        window.focus(&focus, cx);
                    }
                }),
            )
            .child(
                div()
                    .id(SharedString::from(slot.selector("devices-scroll")))
                    .size_full()
                    .overflow_y_scroll()
                    .track_scroll(&page.scroll)
                    .child(
                        div()
                            .p_5()
                            .flex()
                            .flex_col()
                            .gap_5()
                            .child(self.overview_header(&look, &devices, page, cx))
                            .child(chart::activity(
                                &look,
                                &self.devices_overview.history,
                                &shown,
                                Tally::sum(&devices),
                                page.lanes,
                                slot,
                                id,
                                cx,
                            ))
                            .child(totals(&look, &devices))
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap_2()
                                    .child(
                                        div()
                                            .flex()
                                            .flex_wrap()
                                            .items_center()
                                            .gap_3()
                                            .child(look.title("Devices", shown.len()))
                                            .child(div().flex_1())
                                            .child(look.legend()),
                                    )
                                    .child(table::devices(
                                        &look,
                                        &shown,
                                        &self.system_load,
                                        slot,
                                        id,
                                        &page.expanded,
                                        cx,
                                    )),
                            ),
                    ),
            )
            .into_any_element()
    }

    fn overview_header(
        &self,
        look: &Look,
        devices: &[Device],
        page: &super::Page,
        cx: &mut Context<Self>,
    ) -> Div {
        let theme = look.theme;
        let agents = Tally::sum(devices).total();
        let unavailable = self.device_setup_unavailable();
        let noun =
            |n: usize, one: &str, many: &str| format!("{n} {}", if n == 1 { one } else { many });
        div()
            .flex()
            .flex_wrap()
            .items_end()
            .gap_3()
            .child(
                div()
                    .flex_1()
                    .min_w(px(220.))
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(
                        div()
                            .text_size(px(look.ui.size + 7.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("Devices overview"),
                    )
                    .child(div().text_color(rgb(theme.subtext())).child(format!(
                        "{} · {}",
                        noun(devices.len(), "device", "devices"),
                        noun(agents, "agent", "agents")
                    ))),
            )
            // The field gives way before the button wraps under the title.
            .child(
                div()
                    .flex_1()
                    .min_w(px(160.))
                    .max_w(px(320.))
                    .child(page.search.clone()),
            )
            .child(
                div()
                    .id("devices-add")
                    .debug_selector(|| "devices-add".into())
                    .flex_none()
                    .px_3()
                    .py_1()
                    .rounded(px(corners::CONTROL))
                    .bg(rgb(theme.primary()))
                    .text_color(rgb(theme.text_on(theme.primary())))
                    .child("Add Device")
                    .when_some(unavailable, |button, reason| {
                        let (foreground, surface) = (theme.foreground, theme.surface);
                        button.opacity(0.5).tooltip(move |_, cx| {
                            cx.new(|_| crate::usage::Hint {
                                text: reason.into(),
                                foreground,
                                surface,
                            })
                            .into()
                        })
                    })
                    .when(unavailable.is_none(), |button| {
                        button
                            .cursor_pointer()
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.open_device_setup(window, cx);
                            }))
                    }),
            )
    }
}

/// Working, blocked, and idle agents on every device, and how many devices
/// are online.
fn totals(look: &Look, devices: &[Device]) -> Div {
    let theme = look.theme;
    let tally = Tally::sum(devices);
    let online = devices
        .iter()
        .filter(|device| device.link == Link::Online)
        .count();
    let unreachable = devices
        .iter()
        .filter(|device| matches!(device.link, Link::Offline | Link::Connecting))
        .count();
    let cells: [(&str, String, String, Option<AgentStatus>); 4] = [
        (
            "Working",
            tally.working.to_string(),
            format!("of {} agents", tally.total()),
            Some(AgentStatus::Working),
        ),
        (
            "Blocked",
            tally.blocked.to_string(),
            "waiting for you".into(),
            Some(AgentStatus::Blocked),
        ),
        (
            "Idle or done",
            (tally.idle + tally.done).to_string(),
            "ready for work".into(),
            Some(AgentStatus::Idle),
        ),
        (
            "Devices online",
            format!("{online}/{}", devices.len()),
            if unreachable == 0 {
                "all reachable".into()
            } else {
                format!("{unreachable} not connected")
            },
            None,
        ),
    ];
    look.panel()
        .debug_selector(|| "devices-totals".into())
        .flex()
        .flex_wrap()
        .children(
            cells
                .into_iter()
                .enumerate()
                .map(|(index, (title, value, detail, status))| {
                    div()
                        .flex_1()
                        // Narrow enough that all four share a row in a
                        // narrow tab; the detail line truncates first.
                        .min_w(px(100.))
                        .p_3()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .when(index > 0, |cell| {
                            cell.border_l_1().border_color(rgb(theme.active))
                        })
                        .child(
                            div()
                                .text_size(look.small())
                                .text_color(rgb(theme.subtext()))
                                .child(title),
                        )
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_2()
                                .when_some(status, |row, status| row.child(look.dot(status)))
                                .child(
                                    div()
                                        .text_size(px(look.ui.size + 9.))
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .child(value),
                                ),
                        )
                        .child(
                            div()
                                .truncate()
                                .text_size(look.small())
                                .text_color(rgb(theme.muted))
                                .child(detail),
                        )
                }),
        )
}
