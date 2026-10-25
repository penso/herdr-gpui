//! The host header row of the spaces list, and the copy of it pinned over the
//! top of a scrolled list.

use super::{
    HOST_ARROW_WIDTH, STATUS_WIDTH,
    cell::RowState,
    label_text,
    layout::SidebarLook,
    line_height,
    row::removing_dot,
    sticky::{self, HostHeader},
    wash,
};
use crate::{HerdrWindow, endpoint::Endpoint};
use gpui::{prelude::*, *};

/// Where the device runs and how it is reached, named again by its tooltip.
fn origin_mark(
    origin: crate::endpoint::origin::Origin,
    id: String,
    size: f32,
    color: u32,
    theme: &crate::config::Theme,
) -> Stateful<Div> {
    let text = SharedString::from(origin.describe());
    let (foreground, surface) = (theme.foreground, theme.surface);
    div()
        .id(SharedString::from(id.clone()))
        .debug_selector(move || id.clone())
        .size(px(size))
        .flex_none()
        .child(svg().path(origin.icon()).size_full().text_color(rgb(color)))
        .tooltip(move |_, cx| {
            cx.new(|_| crate::usage::Hint {
                text: text.clone(),
                foreground,
                surface,
            })
            .into()
        })
}

impl HerdrWindow {
    /// A host's header row. `pinned` draws the copy that sits over the list
    /// instead of the one inside it: the same row under `sticky-` ids, so the
    /// two never share element state, with an opaque background for the rows
    /// scrolling beneath it.
    pub(super) fn host_row(
        &self,
        endpoint: &Endpoint,
        selected: bool,
        look: SidebarLook,
        width: f32,
        pinned: bool,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let font = &self.config.sidebar;
        let theme = &self.theme;
        let layout = look.density;
        let content_x = look.content_x();
        let host_gap = layout.host_gap();
        // The label yields room to the arrow and to the trailing status dot.
        let host_label_width =
            (look.content_width(width) - HOST_ARROW_WIDTH - 2. * host_gap - STATUS_WIDTH).max(0.);
        let prefix = if pinned { "sticky-" } else { "" };
        let endpoint_id = endpoint.id.clone();
        let collapse_id = endpoint_id.clone();
        let select_id = endpoint_id.clone();
        let menu_id = endpoint_id.clone();
        let removing = self.device_removing(endpoint);
        let endpoint_collapsed = endpoint.collapsed;
        let endpoint_label = endpoint.label.clone();
        let connected = endpoint.live.status.is_connected();
        // Of cloud machines, only Azure's load is read so far.
        let host = crate::usage::Host::load_of(&endpoint.connection.target);
        let reading = host.as_ref().and_then(|host| self.system_load.get(host));
        let load = reading.filter(|_| self.config.show_system_load);
        // Densities with detail lines give the load its own line;
        // compact ones fit gauges between the name and the status, the
        // GPU's among them when the sidebar shows it.
        let load_line = load.filter(|_| layout.workspace_details());
        let gpu_gauge = self.config.show_sidebar_gpu_load
            && reading.is_some_and(|reading| {
                reading
                    .latest()
                    .is_some_and(|sample| sample.gpu.is_some_and(|gpu| gpu.busy.is_some()))
            });
        let gauges = reading
            .filter(|_| !layout.workspace_details())
            .filter(|_| self.config.show_system_load || gpu_gauge)
            .map(|reading| {
                let (width, gauges) = crate::system_load::gauges(
                    reading,
                    theme,
                    super::metrics::glyph_width(font),
                    (font.size * 0.8).round(),
                    self.config.show_system_load,
                    self.config.show_sidebar_gpu_load,
                );
                (width, gauges, reading)
            });
        // The Devices layout counts the agents listed under the header.
        let working = (!self.config.layout.mode.lists_workspaces())
            .then(|| self.working_summary(endpoint))
            .flatten();
        let working_width = working.as_ref().map_or(0., |text| {
            text.chars().count() as f32 * super::metrics::glyph_width(font) + host_gap
        });
        // The removal pulse, the count, and the gauges take their room from
        // the label, not from the status.
        let label_width = (host_label_width
            - if removing {
                STATUS_WIDTH + host_gap
            } else {
                0.
            }
            - working_width
            - gauges.as_ref().map_or(0., |(width, ..)| width + host_gap))
        .max(0.);
        // A host with a GPU adds its own line under CPU and memory, in the
        // densities that have detail lines.
        let gpu_line = host
            .as_ref()
            .filter(|_| self.config.show_sidebar_gpu_load && layout.workspace_details())
            .and_then(|host| {
                let reading = self.system_load.get(host)?;
                let line = crate::system_load::gpu_line(
                    reading,
                    theme,
                    Some(super::metrics::glyph_width(font)),
                    crate::config::status_bar::Detail::Detailed,
                )?;
                Some((line, reading, host.clone()))
            });
        let lines = 1.
            + if load_line.is_some() { 1. } else { 0. }
            + if gpu_line.is_some() { 1. } else { 0. };
        // The layout decides whether headers have a mark; the setting turns it off.
        let origin = (self.config.show_device_icons && self.config.layout.mode.device_icons())
            .then(|| crate::endpoint::origin::Origin::of(&endpoint.connection.target));
        let icon_size = (font.size * 0.9).round();
        let icon_room = origin.map_or(0., |_| icon_size + host_gap);
        let label_width = (label_width - icon_room).max(0.);
        div()
            .id(SharedString::from(format!("{prefix}host-{endpoint_id}")))
            .debug_selector(|| format!("{prefix}host-{endpoint_id}"))
            // The pinned copy repeats the header it covers, which stays listed.
            .when(!pinned, |row| {
                let endpoint = endpoint_id.clone();
                let row = row
                    .role(Role::ListItem)
                    .aria_label(super::accessibility::device_name(
                        &endpoint_label,
                        connected,
                        working.as_deref(),
                    ))
                    .aria_selected(selected)
                    .map(|control| {
                        self.chrome_stop(crate::window::chrome_focus::Region::Sidebar, control)
                    });
                super::accessibility::expandable(
                    row,
                    !endpoint_collapsed,
                    cx.entity().downgrade(),
                    move |this, _, cx| this.toggle_device(&endpoint, cx),
                )
            })
            .h(px(lines * line_height(font)
                + 2. * layout.host_padding()
                + look.chrome_height()))
            .flex_none()
            .relative()
            .flex()
            .flex_col()
            .justify_center()
            .px(px(content_x))
            // Rows scrolling underneath show through anything translucent, so
            // the copy paints the sidebar's own background. It stops hover and
            // clicks reaching those rows but not the wheel, which must keep
            // scrolling the list while the pointer rests on the pinned header.
            .when(pinned, |row| {
                row.bg(rgb(theme.sidebar_background()))
                    .block_mouse_except_scroll()
            })
            // The host's colour wash goes with its highlight, on the pinned
            // copy too, so the header keeps its colour while it scrolls.
            // Hosts mark selection only; they do not join the rows'
            // hover group.
            // A carried header is a card lifted over the headers it passes.
            .map(|row| {
                let mark = wash::HostMark::resolve(
                    &self.config.sidebar_style,
                    &endpoint.label,
                    selected,
                    theme,
                );
                let key = format!("{prefix}host-{endpoint_id}");
                if !pinned
                    && self
                        .device_drag
                        .as_ref()
                        .is_some_and(|drag| drag.carries(&endpoint.id))
                {
                    mark.apply(row, &key, &look).child(look.lifted(
                        &format!("host-{endpoint_id}"),
                        selected,
                        theme,
                    ))
                } else {
                    let layer = look.highlight(
                        &key,
                        RowState {
                            selected,
                            ..RowState::default()
                        },
                        theme,
                    );
                    look.washed(row, &key, mark, layer)
                }
            })
            .text_color(rgb(if endpoint.enabled {
                theme.foreground
            } else {
                theme.muted
            }))
            .cursor_pointer()
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                    cx.stop_propagation();
                    this.open_host_menu(&menu_id, event.position, window, cx);
                    this.menu.opening_right_click = this.menu.page == Some(crate::menu::Page::Host);
                }),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(host_gap))
                    .child(
                        div()
                            .id(SharedString::from(format!(
                                "{prefix}collapse-host-{endpoint_id}"
                            )))
                            .when(pinned, |arrow| {
                                arrow.debug_selector(|| {
                                    format!("{prefix}collapse-host-{endpoint_id}")
                                })
                            })
                            .w(px(HOST_ARROW_WIDTH))
                            .flex_none()
                            .child(label_text(if endpoint.collapsed {
                                "\u{25b8}"
                            } else {
                                "\u{25be}"
                            }))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                cx.stop_propagation();
                                this.toggle_device(&collapse_id, cx);
                            })),
                    )
                    .when_some(origin, |row, origin| {
                        row.child(origin_mark(
                            origin,
                            format!("{prefix}host-origin-{endpoint_id}"),
                            icon_size,
                            if endpoint.enabled {
                                theme.subtext()
                            } else {
                                theme.muted
                            },
                            theme,
                        ))
                    })
                    .when(removing, |row| {
                        row.child(removing_dot("host-removing", theme))
                    })
                    .child(
                        div()
                            .debug_selector(|| format!("{prefix}host-label-{endpoint_id}"))
                            // As with workspace labels, avoid zero-basis text measurement.
                            .w(px(label_width))
                            .flex_none()
                            .overflow_hidden()
                            .child(
                                div()
                                    .w(px(label_width))
                                    .truncate()
                                    .child(label_text(&endpoint.label)),
                            ),
                    )
                    .when_some(working, |row, text| {
                        row.child(
                            div()
                                .debug_selector(|| format!("{prefix}host-working-{endpoint_id}"))
                                .flex_none()
                                .text_color(rgb(theme.muted))
                                .child(text),
                        )
                    })
                    .when_some(
                        gauges.zip(host.clone()),
                        |row, ((_, gauges, reading), host)| {
                            row.child(
                                div()
                                    .id(SharedString::from(format!(
                                        "{prefix}host-load-{endpoint_id}"
                                    )))
                                    .flex_none()
                                    .child(gauges)
                                    .tooltip(crate::system_load::tooltip(reading, &host, theme)),
                            )
                        },
                    )
                    .child(
                        div()
                            .debug_selector(|| format!("{prefix}host-status-{endpoint_id}"))
                            .size(px(STATUS_WIDTH))
                            .flex_none()
                            .rounded_full()
                            .bg(rgb(if endpoint.live.status.is_connected() {
                                crate::menu::online(theme)
                            } else {
                                theme.muted
                            })),
                    ),
            )
            .when_some(load_line.zip(host), |row, (reading, host)| {
                row.child(
                    div()
                        .id(SharedString::from(format!(
                            "{prefix}host-load-{endpoint_id}"
                        )))
                        .h(px(line_height(font)))
                        .flex()
                        .items_center()
                        .pl(px(HOST_ARROW_WIDTH + host_gap + icon_room))
                        .overflow_hidden()
                        .child(crate::system_load::line(
                            reading,
                            theme,
                            Some(super::metrics::glyph_width(font)),
                            crate::config::status_bar::Detail::Detailed,
                        ))
                        .tooltip(crate::system_load::tooltip(reading, &host, theme)),
                )
            })
            .when_some(gpu_line, |row, (line, reading, host)| {
                row.child(
                    div()
                        .id(SharedString::from(format!(
                            "{prefix}host-gpu-{endpoint_id}"
                        )))
                        .debug_selector(|| format!("{prefix}host-gpu-{endpoint_id}"))
                        .h(px(line_height(font)))
                        .flex()
                        .items_center()
                        .pl(px(HOST_ARROW_WIDTH + host_gap + icon_room))
                        .overflow_hidden()
                        .child(line)
                        .tooltip(crate::system_load::tooltip(reading, &host, theme)),
                )
            })
            .on_click(cx.listener(move |this, _, window, cx| {
                this.select_endpoint(&select_id, cx);
                window.focus(&this.focus, cx);
            }))
    }

    /// Folds or unfolds a device's header, from its arrow or a screen reader.
    fn toggle_device(&mut self, endpoint_id: &str, cx: &mut Context<Self>) {
        if let Some(endpoint) = self.endpoints.iter_mut().find(|e| e.id == endpoint_id) {
            endpoint.collapsed = !endpoint.collapsed;
        }
        cx.notify();
    }

    /// Folds or unfolds a repository's linked worktrees on one device.
    pub(super) fn toggle_repository(
        &mut self,
        endpoint_id: &str,
        key: &String,
        cx: &mut Context<Self>,
    ) {
        let collapsed = if endpoint_id == crate::endpoint::LOCAL {
            &mut self.collapsed_repos
        } else if let Some(endpoint) = self.endpoints.iter_mut().find(|e| e.id == endpoint_id) {
            &mut endpoint.collapsed_repos
        } else {
            return;
        };
        if !collapsed.remove(key) {
            collapsed.insert(key.clone());
        }
        cx.notify();
    }

    /// `2/5`: a connected host's working agents of all its agents.
    fn working_summary(&self, endpoint: &Endpoint) -> Option<String> {
        let index = self.endpoints.iter().position(|e| e.id == endpoint.id)?;
        let live = if index == self.selected_endpoint {
            &self.live
        } else {
            &endpoint.live
        };
        let tally = crate::devices_overview::online_tally(endpoint, live)?;
        Some(format!("{}/{}", tally.working, tally.total()))
    }

    /// Each host header the spaces list last laid out, at its current scroll
    /// offset. `rows` pairs each host's endpoint index with its header's child
    /// position in the list. Positions come from the previous frame's layout,
    /// as the handle reports them, so there are none until a frame has
    /// measured the list.
    pub(super) fn host_headers(&self, rows: &[(usize, usize)]) -> Vec<HostHeader> {
        let scroll = &self.sidebar_scroll[0];
        if scroll.bounds().size.height <= px(0.) {
            return Vec::new();
        }
        let list_top = scroll.bounds().top();
        let offset = scroll.offset().y;
        rows.iter()
            .filter_map(|&(host, child)| {
                let bounds = scroll.bounds_for_item(child)?;
                Some(HostHeader {
                    host,
                    top: f32::from(bounds.top() + offset - list_top),
                    height: f32::from(bounds.size.height),
                })
            })
            .collect()
    }

    /// The copy of `pinned`'s header held over the top of the spaces list.
    pub(super) fn pinned_host_row(
        &self,
        pinned: HostHeader,
        look: SidebarLook,
        width: f32,
        cx: &mut Context<Self>,
    ) -> Option<Stateful<Div>> {
        let endpoint = self.endpoints.get(pinned.host)?;
        let row = self.host_row(
            endpoint,
            pinned.host == self.selected_endpoint,
            look,
            width,
            true,
            cx,
        );
        Some(row.absolute().top(px(pinned.top)).left_0().right_0())
    }

    /// Moves the spaces list down when `row`, revealed last frame, landed
    /// under the pinned header: GPUI reveals a row from above at the list's
    /// top edge, which the pinned header covers. Returns whether it moved.
    pub(super) fn uncover_revealed(&self, row: usize, headers: &[HostHeader]) -> bool {
        let scroll = &self.sidebar_scroll[0];
        let Some(bounds) = scroll.bounds_for_item(row) else {
            return false;
        };
        let offset = scroll.offset();
        let top = f32::from(bounds.top() + offset.y - scroll.bounds().top());
        let down = sticky::cover(headers) - top;
        if down <= 0. {
            return false;
        }
        scroll.set_offset(point(offset.x, (offset.y + px(down)).min(px(0.))));
        true
    }
}

impl HerdrWindow {
    /// Whether `endpoint` is being removed: from the sidebar, or its machine
    /// deleted at Azure from Settings or by auto-stop.
    pub(crate) fn device_removing(&self, endpoint: &Endpoint) -> bool {
        self.menu.removing_devices.contains(&endpoint.id)
            || destroying_at_azure(&endpoint.connection.target)
    }
}

/// Whether the machine behind `target` is being deleted at Azure, by Remove
/// from here or Settings, or by auto-stop: its header pulses as for removal.
fn destroying_at_azure(target: &herdr_client::ConnectTarget) -> bool {
    #[cfg(feature = "azure")]
    if let herdr_client::ConnectTarget::Cloud {
        provider: herdr_client::CloudProvider::Azure,
        id,
        ..
    } = target
    {
        return crate::azure::destroy::destroying(id);
    }
    let _ = target;
    false
}
