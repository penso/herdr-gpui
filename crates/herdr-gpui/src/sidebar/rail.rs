//! The collapsed sidebar's compact rail, as Herdr's default
//! `ui.sidebar_collapsed_mode = "compact"` draws it: per host a small header
//! box, then a status mark per workspace and per agent. The header box stays
//! its own shape and wears the workspace badge's chrome: the same fill,
//! stroke, and corner as the selected workspace number, one step quieter on
//! every other host. Workspace badges and agent rows share one borderless
//! grid, centered on that box, and the selection highlight is the only other
//! box: one width, wrapped around the glyph, the icon, and the dot. Every
//! mark navigates where its expanded row would, and its tooltip names what
//! the narrow column cannot.

use super::{
    agent_name,
    agents::{Indicators, agent_place, state_label, status_indicator, status_text},
    cell::RowState,
    glyph_width, label_text,
    layout::{self, Highlight, SidebarLook},
    line_height, visible_workspace_entries,
    wash::HostMark,
    workspaces::workspace_label,
};
use crate::{
    Command, HerdrWindow, NavigationTarget,
    config::{FontConfig, Theme},
    fonts::StyledFont,
    herdr_settings::SidebarCollapsedMode,
};
use gpui::{prelude::*, *};
use herdr_client::protocol::{AgentStatus, ClientShellAgent};
use std::borrow::Cow;

/// Wide enough for the shared mark grid: a two-digit workspace number, an
/// agent icon, and a drawn status icon, plus even padding inside the highlight.
pub(crate) const RAIL_WIDTH: f32 = 64.;
const HOST_MARK: f32 = 22.;
const HOST_BORDER: f32 = 1.;
const HOST_DOT: f32 = 6.;
/// Sidebar-colored ring around the status dot, so the header stroke cannot
/// cut through it.
const HOST_DOT_RING: f32 = 2.;
/// Room between the header stroke and that ring. The ring hangs outside the
/// box; the gap is the sidebar, and the stroke never meets the dot.
const HOST_DOT_GAP: f32 = 2.;
const AGENT_ICON: f32 = 14.;
const MARK_GAP: f32 = 4.;
/// Padding the mark highlight keeps around the shared column grid.
const RAIL_PAD: f32 = 4.;

/// What the sidebar column shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SidebarMode {
    Expanded,
    Rail,
    Hidden,
}

impl SidebarMode {
    /// The column's width, under the same terminal reserve the expanded
    /// sidebar keeps, so a narrow window never loses its terminal to it.
    pub(crate) fn width(self, preferred: Option<f32>, window_width: f32) -> Option<f32> {
        match self {
            Self::Expanded => Some(super::sidebar_width(preferred, window_width)),
            // A window too narrow for any rail gets none rather than a border.
            Self::Rail => Some(RAIL_WIDTH.min(window_width - 240.)).filter(|width| *width > 0.),
            Self::Hidden => None,
        }
    }
}

impl HerdrWindow {
    /// Zen mode hides the column whatever the toggle says; otherwise it is
    /// [`Self::chrome_sidebar_mode`].
    pub(crate) fn sidebar_mode(&self) -> SidebarMode {
        if self.zen_layout() {
            return SidebarMode::Hidden;
        }
        self.chrome_sidebar_mode()
    }

    /// The column outside zen mode, which zen mode slides away. Collapsing
    /// follows the local daemon's `ui.sidebar_collapsed_mode`, also while an
    /// SSH host is shown, as Herdr keeps presentation local. Before shared
    /// settings load, Herdr's default rail applies.
    pub(crate) fn chrome_sidebar_mode(&self) -> SidebarMode {
        if self.sidebar_visible || self.sidebar_slide.closing() {
            return SidebarMode::Expanded;
        }
        match self
            .settings
            .shared
            .as_ref()
            .map_or(SidebarCollapsedMode::Compact, |shared| {
                shared.sidebar_collapsed_mode
            }) {
            SidebarCollapsedMode::Compact => SidebarMode::Rail,
            SidebarCollapsedMode::Hidden => SidebarMode::Hidden,
        }
    }

    pub(crate) fn toggle_sidebar(&mut self) {
        self.sidebar_visible = !self.sidebar_visible;
        self.sidebar_slide
            .set(self.sidebar_visible, crate::motion::now());
        self.sidebar_start_pending = false;
    }

    /// Applies `ui.sidebar_start_collapsed` once, from the first shared
    /// settings to load. Later reloads, and a toggle made before the first
    /// load arrives, leave the user's choice alone, as Herdr reads the setting
    /// at startup only.
    pub(crate) fn apply_sidebar_start(&mut self) {
        if !std::mem::take(&mut self.sidebar_start_pending) {
            return;
        }
        if self
            .settings
            .shared
            .as_ref()
            .is_some_and(|shared| shared.sidebar_start_collapsed)
        {
            self.sidebar_visible = false;
        }
    }

    pub(crate) fn render_rail(
        &self,
        indicators: Indicators,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let width = SidebarMode::Rail
            .width(None, f32::from(window.viewport_size().width))
            .unwrap_or(0.);
        let look = layout::for_config(&self.config, self.fill());
        let theme = &self.theme;
        let hint = Hint::new(theme);
        let multi = self.endpoints.len() > 1;
        // A large sidebar font would make the columns wider than the rail and
        // clip the status icons, so the rail steps the font down until they fit.
        let fitted = rail_font(&self.config.sidebar, indicators, multi);
        let font = &fitted;
        let columns = rail_columns(font, indicators, multi);
        let split = self.sidebar_split.unwrap_or(0.5).clamp(0.1, 0.9);
        let mut spaces = div()
            .id("rail-spaces")
            .debug_selector(|| "rail-spaces".into())
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .overflow_y_scroll();
        let mut agents = div()
            .id("rail-agents")
            .debug_selector(|| "rail-agents".into())
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .overflow_y_scroll();
        for (endpoint_index, endpoint) in self.displayed_devices() {
            if !self.device_visible(&endpoint.id) {
                continue;
            }
            let selected = endpoint_index == self.selected_endpoint;
            let endpoint_id = endpoint.id.clone();
            let initial = host_initial(&endpoint.label);
            if multi {
                let select_id = endpoint_id.clone();
                let menu_id = endpoint_id.clone();
                let connected = endpoint.live.status.is_connected();
                let text = if endpoint.collapsed {
                    format!("{} (collapsed)", endpoint.label)
                } else {
                    endpoint.label.clone()
                };
                let box_id = endpoint_id.clone();
                let chrome = host_chrome(selected, look, theme);
                spaces = spaces.child(
                    // No row highlight here. The letter's own box is the
                    // header; the selection highlight belongs to the rows
                    // beneath it, and painting both made two boxes clash.
                    host_header(font, look)
                        .child(
                            div()
                                .debug_selector(move || format!("rail-host-box-{box_id}"))
                                .relative()
                                .size(px(HOST_MARK))
                                .flex_none()
                                .flex()
                                .items_center()
                                .justify_center()
                                .rounded(px(chrome.radius))
                                .border_1()
                                .border_color(chrome.border)
                                .bg(chrome.fill)
                                .text_color(chrome.letter)
                                .child(label_text(&initial))
                                .child(host_status(
                                    format!("rail-host-status-{endpoint_id}"),
                                    if connected {
                                        crate::menu::online(theme)
                                    } else {
                                        theme.muted
                                    },
                                    theme.sidebar_background(),
                                    width,
                                )),
                        )
                        .id(SharedString::from(format!("rail-host-{endpoint_id}")))
                        .debug_selector(|| format!("rail-host-{endpoint_id}"))
                        .cursor_pointer()
                        .tooltip(hint.with(text))
                        .on_mouse_down(
                            MouseButton::Right,
                            cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                                cx.stop_propagation();
                                this.open_host_menu(&menu_id, event.position, window, cx);
                                this.menu.opening_right_click =
                                    this.menu.page == Some(crate::menu::Page::Host);
                            }),
                        )
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.select_endpoint(&select_id, cx);
                            window.focus(&this.focus, cx);
                        })),
                );
            }
            if multi && endpoint.collapsed {
                continue;
            }
            let live = if selected { &self.live } else { &endpoint.live };
            let Some(snapshot) = &live.snapshot else {
                continue;
            };
            let collapsed_repos = if endpoint_index == 0 {
                &self.collapsed_repos
            } else {
                &endpoint.collapsed_repos
            };
            let host = multi.then_some(endpoint.label.as_str());
            for (index, indented, _) in
                visible_workspace_entries(&snapshot.workspaces, collapsed_repos)
            {
                let workspace = &snapshot.workspaces[index];
                let id = workspace.workspace_id.clone();
                let key = format!("rail-workspace-{endpoint_id}-{id}");
                let navigate_endpoint = endpoint_id.clone();
                let context_endpoint = endpoint_id.clone();
                let context_id = id.clone();
                let focused = selected && workspace.focused;
                let number = workspace.number.to_string();
                let marks = div()
                    .flex()
                    .items_center()
                    .gap(px(columns.gap))
                    .child(mark_slot(
                        format!("rail-mark-{key}"),
                        columns.badge,
                        columns.height,
                        if focused {
                            theme.foreground
                        } else {
                            theme.muted
                        },
                        &number,
                    ))
                    .when_some(columns.icon, |marks, icon| {
                        marks.child(div().flex_none().w(px(icon)).h(px(columns.height)))
                    })
                    .child(dot_slot(
                        format!("rail-dot-{key}"),
                        columns.dot,
                        columns.height,
                        indicator(workspace.agent_status, font, indicators),
                    ));
                spaces = spaces.child(
                    aligned_row(
                        &key,
                        RowState {
                            selected: focused,
                            ..RowState::default()
                        },
                        font,
                        look,
                        theme,
                        columns,
                        marks,
                    )
                    .id(SharedString::from(key.clone()))
                    .debug_selector(|| key)
                    .cursor_pointer()
                    .tooltip(hint.with(describe(
                        host,
                        workspace_label(workspace, indented),
                        None,
                        status_word(workspace.agent_status),
                    )))
                    .on_mouse_down(
                        MouseButton::Right,
                        cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                            cx.stop_propagation();
                            if this.navigate_endpoint(
                                &context_endpoint,
                                NavigationTarget::Workspace(&context_id),
                                cx,
                            ) {
                                this.open_workspace_menu(&context_id, event.position, window, cx);
                                this.menu.opening_right_click =
                                    this.menu.page == Some(crate::menu::Page::Workspace);
                            }
                        }),
                    )
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.navigate_endpoint(
                            &navigate_endpoint,
                            NavigationTarget::Workspace(&id),
                            cx,
                        );
                        window.focus(&this.focus, cx);
                    })),
                );
            }
        }
        // Agents come after every host: the panel interleaves them.
        let panel = if self.config.show_agents {
            self.panel_agents()
        } else {
            Vec::new()
        };
        for (index, agent) in panel {
            let Some(endpoint) = self.endpoints.get(index) else {
                continue;
            };
            // A collapsed host folds its agents away with its workspaces.
            if multi && endpoint.collapsed {
                continue;
            }
            let selected = index == self.selected_endpoint;
            let live = if selected { &self.live } else { &endpoint.live };
            let Some(snapshot) = live.snapshot.as_deref() else {
                continue;
            };
            let endpoint_id = &endpoint.id;
            let host = multi.then_some(endpoint.label.as_str());
            let id = agent.pane_id.clone();
            let key = format!("rail-agent-{endpoint_id}-{id}");
            let navigate_endpoint = endpoint_id.clone();
            let focused = selected && agent.focused;
            let place = agent_place(agent, snapshot);
            let place = place.map(|(workspace, tab)| match tab {
                Some(tab) => format!("{workspace} / {tab}"),
                None => workspace.to_owned(),
            });
            let icon = AGENT_ICON.min(font.size + 1.);
            let icon_color = if focused {
                theme.foreground
            } else {
                theme.muted
            };
            let icon_path = crate::icons::AgentIcon::from_identity(agent.agent.as_deref()).path();
            // The host letter shares the workspace number's column. With one
            // host there is no letter, so the icon takes that column and still
            // starts on the badge's left edge.
            let badge = if multi {
                mark_slot(
                    format!("rail-mark-{key}"),
                    columns.badge,
                    columns.height,
                    theme.muted,
                    &host_initial(&endpoint.label),
                )
            } else {
                div()
                    .debug_selector(|| format!("rail-mark-{key}"))
                    .flex_none()
                    .w(px(columns.badge))
                    .h(px(columns.height))
                    .flex()
                    .items_center()
                    .child(agent_icon(
                        format!("rail-agent-icon-{endpoint_id}-{id}"),
                        icon,
                        icon_color,
                        icon_path,
                    ))
            };
            let marks = div()
                .flex()
                .items_center()
                .gap(px(columns.gap))
                .child(badge)
                .when(multi, |marks| {
                    marks.child(agent_icon(
                        format!("rail-agent-icon-{endpoint_id}-{id}"),
                        icon,
                        icon_color,
                        icon_path,
                    ))
                })
                .child(dot_slot(
                    format!("rail-dot-{key}"),
                    columns.dot,
                    columns.height,
                    indicator(agent.agent_status, font, indicators),
                ));
            agents = agents.child(
                aligned_row(
                    &key,
                    RowState {
                        selected: focused,
                        ..RowState::default()
                    },
                    font,
                    look,
                    theme,
                    columns,
                    marks,
                )
                .id(SharedString::from(key.clone()))
                .debug_selector(|| key)
                .cursor_pointer()
                .tooltip(hint.with(describe(
                    host,
                    agent_name(agent),
                    place.as_deref(),
                    agent_status_word(agent).as_deref(),
                )))
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.navigate_endpoint(&navigate_endpoint, NavigationTarget::Pane(&id), cx);
                    window.focus(&this.focus, cx);
                })),
            );
        }
        div()
            .id("sidebar-rail")
            .debug_selector(|| "sidebar-rail".into())
            .relative()
            .w(px(width))
            .flex_none()
            .h_full()
            .min_h_0()
            .overflow_hidden()
            .flex()
            .flex_col()
            .text_font(font)
            .text_size(px(font.size))
            .line_height(px(line_height(font)))
            .text_color(rgb(theme.foreground))
            .bg(self.fill().panel(theme.sidebar_background()))
            .border_r_1()
            .border_color(rgb(theme.active))
            // No expand control of its own: the sidebar toggle is always on
            // screen, over the rail beside the activity bar, else in the
            // header or leading the tab row.
            .child(
                div()
                    .debug_selector(|| "rail-spaces-section".into())
                    .flex()
                    .flex_col()
                    .flex_1()
                    .map(|mut section| {
                        section.style().flex_grow =
                            Some(if self.config.show_agents { split } else { 1. });
                        section
                    })
                    .min_h_0()
                    .overflow_hidden()
                    .child(spaces),
            )
            .when(self.config.show_agents, |rail| {
                rail.child(div().flex_none().h(px(1.)).bg(rgb(theme.active)))
                    .child(
                        div()
                            .debug_selector(|| "rail-agents-section".into())
                            .flex()
                            .flex_col()
                            .flex_1()
                            .map(|mut section| {
                                section.style().flex_grow = Some(1. - split);
                                section
                            })
                            .min_h_0()
                            .overflow_hidden()
                            .child(agents),
                    )
            })
            .child(
                rail_button("rail-new-workspace", font, look, theme)
                    .text_color(rgb(theme.muted))
                    .hover(|s| s.text_color(rgb(theme.foreground)))
                    .tooltip(hint.with("New workspace"))
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.command(Command::Workspace, window, cx);
                    }))
                    .child(label_text("+")),
            )
    }
}

/// The columns a workspace badge and an agent row share. `icon` is set only
/// when several hosts are listed: the agent row then has a host letter, and
/// workspace rows keep an empty slot of the same width so the status dots
/// stay in one column.
#[derive(Clone, Copy)]
struct Columns {
    badge: f32,
    dot: f32,
    gap: f32,
    height: f32,
    icon: Option<f32>,
}

impl Columns {
    fn width(self) -> f32 {
        let gaps = self.gap * if self.icon.is_some() { 2. } else { 1. };
        self.badge + self.icon.unwrap_or(0.) + gaps + self.dot
    }
}

fn rail_columns(font: &FontConfig, indicators: Indicators, multi: bool) -> Columns {
    let glyph = glyph_width(font);
    let icon = AGENT_ICON.min(font.size + 1.);
    Columns {
        badge: (glyph * 2.).max(if multi { glyph } else { icon }),
        dot: indicators.width(font),
        gap: MARK_GAP,
        height: line_height(font).max(icon),
        icon: multi.then_some(icon),
    }
}

/// The widest the column grid may be: the rail less an outline's pixel on
/// each side.
const RAIL_GRID: f32 = RAIL_WIDTH - 2.;

/// The smallest font the rail steps down to.
const RAIL_FONT_MIN: f32 = 8.;

/// `font`, stepped down in half pixels until the column grid fits the rail.
/// A font that already fits is returned as is.
fn rail_font(font: &FontConfig, indicators: Indicators, multi: bool) -> FontConfig {
    let mut fitted = font.clone();
    while rail_columns(&fitted, indicators, multi).width() > RAIL_GRID
        && fitted.size > RAIL_FONT_MIN
    {
        fitted.size = (fitted.size - 0.5).max(RAIL_FONT_MIN);
    }
    fitted
}

/// How much room the highlight can give the grid on every side and still sit
/// inside the rail and the row. Outline borders take a pixel of that room.
fn rail_pad(look: SidebarLook, row_h: f32, columns: Columns) -> f32 {
    let border = match look.density.highlight {
        Highlight::Outline => 1.,
        Highlight::Fill => 0.,
    };
    let room_x = (RAIL_WIDTH - columns.width()) / 2. - border;
    let room_y = (row_h - columns.height) / 2. - border;
    RAIL_PAD.min(room_x).min(room_y).max(0.)
}

/// A workspace or agent row whose highlight wraps the shared column grid
/// with the same padding on every side. The grid is centered in the rail, so
/// every such row shares one left edge and one width.
fn aligned_row(
    key: &str,
    state: RowState,
    font: &FontConfig,
    look: SidebarLook,
    theme: &Theme,
    columns: Columns,
    marks: Div,
) -> Div {
    let row_h = look.row_height(line_height(font).max(HOST_MARK));
    let pad = rail_pad(look, row_h, columns);
    look.hover_group(
        div()
            .relative()
            .flex_none()
            .h(px(row_h))
            .w_full()
            .flex()
            .items_center()
            .justify_center()
            .overflow_hidden(),
    )
    .child(
        look.paint_highlight(div().flex_none().p(px(pad)), key, state, theme)
            .child(marks),
    )
}

/// The badge column: a workspace number or, when several hosts are listed,
/// the agent's host letter. Fixed width, so both kinds of row share its left edge.
fn mark_slot(selector: String, width: f32, height: f32, color: u32, text: &str) -> Div {
    div()
        .debug_selector(move || selector)
        .flex_none()
        .w(px(width))
        .h(px(height))
        .flex()
        .items_center()
        .text_color(rgb(color))
        .child(label_text(text))
}

fn agent_icon(selector: String, size: f32, color: u32, path: &'static str) -> Div {
    div()
        .debug_selector(move || selector)
        .flex_none()
        .size(px(size))
        .flex()
        .items_center()
        .justify_center()
        .child(crate::icons::icon(path, size, color))
}

fn dot_slot(selector: String, width: f32, height: f32, mark: Div) -> Div {
    div()
        .debug_selector(move || selector)
        .flex_none()
        .w(px(width))
        .h(px(height))
        .flex()
        .items_center()
        .justify_center()
        .child(mark)
}

/// The status dot beside the header's bottom-right corner. A sidebar-colored
/// ring, then the dot, hangs outside the box and within its height, so the
/// stroke does not cross the dot and a compact row does not clip it. A rail
/// too narrow for that gap keeps the ring inside the box instead. The same
/// place is used for the accented host and the dimmed ones.
fn host_status(selector: String, color: u32, ring: u32, rail_width: f32) -> Div {
    let ring_selector = selector.replacen("rail-host-status-", "rail-host-ring-", 1);
    let ring_size = HOST_DOT + 2. * HOST_DOT_RING;
    div()
        .debug_selector(move || ring_selector)
        .absolute()
        .right(px(host_status_right(rail_width, ring_size)))
        .bottom(px(HOST_BORDER))
        .size(px(ring_size))
        .flex()
        .items_center()
        .justify_center()
        .rounded_full()
        .bg(rgb(ring))
        .child(
            div()
                .debug_selector(move || selector)
                .size(px(HOST_DOT))
                .flex_none()
                .rounded_full()
                .bg(rgb(color)),
        )
}

/// `right` inset of the status ring. Negative when the ring hangs past the
/// box. Absolute `right` is the distance from the padding edge, and the
/// border is subtracted again: `left = mark - border - size - right`.
fn host_status_right(rail_width: f32, ring_size: f32) -> f32 {
    let content = (rail_width - HOST_BORDER).max(HOST_MARK);
    let side = (content - HOST_MARK) / 2.;
    if side + 0.5 >= HOST_DOT_GAP + ring_size {
        return -(HOST_BORDER + ring_size + HOST_DOT_GAP);
    }
    // The column cannot hold the box and a dot beside it. Sit inside.
    HOST_BORDER
}

/// A host header: the letter box, centered on the same line as the row grid
/// below it. Smaller than the selection highlight, and painted with the
/// workspace badge's chrome rather than a second copy of that highlight.
fn host_header(font: &FontConfig, look: SidebarLook) -> Div {
    div()
        .relative()
        .flex_none()
        .h(px(look.row_height(line_height(font).max(HOST_MARK))))
        .w_full()
        .flex()
        .items_center()
        .justify_center()
        .overflow_hidden()
}

/// Paint for one host header. The box is [`SidebarLook::badge_chrome`]: the
/// selected workspace's badge when this host holds that workspace, and the
/// same washes one step quieter otherwise. The letter stays the foreground
/// on the active host and the muted token on the others.
struct HostChrome {
    border: Rgba,
    fill: Rgba,
    letter: Rgba,
    radius: f32,
}

fn host_chrome(active: bool, look: SidebarLook, theme: &Theme) -> HostChrome {
    let badge = look.badge_chrome(active, theme);
    HostChrome {
        border: badge.border,
        fill: badge.fill,
        letter: rgb(if active {
            theme.foreground
        } else {
            theme.muted
        }),
        radius: badge.radius,
    }
}

/// A rail row: centered marks over the active layout's highlight, so the rail
/// keeps the expanded rows' look in every sidebar layout.
fn rail_row(
    key: &str,
    state: RowState,
    font: &FontConfig,
    look: SidebarLook,
    theme: &Theme,
) -> Div {
    look.mark(
        div()
            .relative()
            .flex_none()
            .h(px(look.row_height(line_height(font).max(HOST_MARK))))
            .flex()
            .items_center()
            .justify_center()
            .overflow_hidden(),
        key,
        (state, HostMark::default()),
        theme,
    )
}

fn rail_button(
    id: &'static str,
    font: &FontConfig,
    look: SidebarLook,
    theme: &Theme,
) -> Stateful<Div> {
    rail_row(id, RowState::default(), font, look, theme)
        .id(id)
        .debug_selector(move || id.into())
        .cursor_pointer()
}

/// The indicator centered on its line, whichever style draws it. A drawn
/// icon a little taller than the line spills evenly into the row's padding.
fn indicator(status: AgentStatus, font: &FontConfig, indicators: Indicators) -> Div {
    div()
        .flex_none()
        .h(px(line_height(font)))
        .flex()
        .items_center()
        .child(status_indicator(status, font, indicators))
}

/// The host's initial, as Herdr marks a machine in its rail.
pub(super) fn host_initial(label: &str) -> String {
    label
        .trim()
        .chars()
        .next()
        .map_or_else(|| "?".into(), |initial| initial.to_uppercase().collect())
}

/// The status word a tooltip names, none for an unknown status.
fn status_word(status: AgentStatus) -> Option<&'static str> {
    (status != AgentStatus::Unknown).then(|| status_text(status))
}

/// An agent's status word: its integration's state label when it set one.
fn agent_status_word(agent: &ClientShellAgent) -> Option<Cow<'static, str>> {
    let word = state_label(agent, status_word(agent.agent_status).unwrap_or(""));
    (!word.is_empty()).then_some(word)
}

/// A rail mark's tooltip: what the expanded row would have spelled out.
/// `status` is the word for the mark's status: an agent's own state label
/// when its integration set one.
pub(super) fn describe(
    host: Option<&str>,
    name: &str,
    place: Option<&str>,
    status: Option<&str>,
) -> SharedString {
    let mut text = String::new();
    if let Some(host) = host {
        text.push_str(host);
        text.push_str(": ");
    }
    text.push_str(name);
    if let Some(place) = place {
        text.push_str(" in ");
        text.push_str(place);
    }
    if let Some(status) = status {
        text.push_str(" (");
        text.push_str(status);
        text.push(')');
    }
    text.into()
}

#[derive(Clone, Copy)]
struct Hint {
    foreground: u32,
    surface: u32,
}

impl Hint {
    fn new(theme: &Theme) -> Self {
        Self {
            foreground: theme.foreground,
            surface: theme.surface,
        }
    }

    fn with(
        self,
        text: impl Into<SharedString>,
    ) -> impl Fn(&mut Window, &mut App) -> AnyView + 'static {
        let text = text.into();
        move |_, cx| {
            cx.new(|_| crate::usage::Hint {
                text: text.clone(),
                foreground: self.foreground,
                surface: self.surface,
            })
            .into()
        }
    }
}

#[cfg(test)]
mod tests;
