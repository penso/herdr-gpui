//! The collapsed sidebar: a narrow rail with one cell per workspace and one
//! status dot per agent. Cells keep the full sidebar's order, selection, and
//! clicks; the names they no longer have room for move into tooltips.

use super::{
    DEVICE_FOOTER_HEIGHT, RowBadge,
    agents::{agent_labels, status_style},
    layout::{self, SidebarLook},
    metrics::{RAIL_CELL, RAIL_DOT_CELL, RAIL_WIDTH},
    render::{Drops, drag_capture},
    row::{RowIcon, github_mark, label_text},
    sorted_agents, visible_workspace_entries,
    workspaces::{workspace_badge, workspace_label},
};
use crate::{
    Command, HerdrWindow, NavigationTarget,
    config::Theme,
    endpoint::Endpoint,
    fonts::StyledFont,
    menu::{CONNECTED, Hint},
};
use gpui::{prelude::*, *};
use herdr_client::protocol::{AgentStatus, ClientShellWorkspace};

/// Room the pull request badge keeps from each side of the cell.
const PR_MARGIN: f32 = 6.;
/// The badge's own horizontal padding and border, inside that margin.
const PR_PADDING: f32 = 4.;
const PR_HEIGHT: f32 = 16.;
/// Numbers print smaller than labels; the badge color already says what they are.
const PR_TEXT: f32 = 11.;
/// Corner marks sit this far in from the cell's edges.
const MARK_INSET: f32 = 3.;

impl HerdrWindow {
    pub(super) fn render_rail(&self, cx: &mut Context<Self>) -> Stateful<Div> {
        let look = layout::for_mode(self.config.layout.mode);
        let font = &self.config.sidebar;
        let theme = &self.theme;
        let split = self.sidebar_split.unwrap_or(0.5).clamp(0.1, 0.9);
        let mut spaces = div()
            .id("spaces-scroll")
            .debug_selector(|| "spaces-scroll".into())
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .track_scroll(&self.sidebar_scroll[0]);
        let mut agents = div()
            .id("agents-scroll")
            .debug_selector(|| "agents-scroll".into())
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .track_scroll(&self.sidebar_scroll[1]);
        let multi = self.endpoints.len() > 1;
        let mut rows = [0usize; 2];
        let mut highlighted = [None; 2];
        for (endpoint_index, endpoint) in self.endpoints.iter().enumerate() {
            if !self.device_visible(&endpoint.id) {
                continue;
            }
            let selected = endpoint_index == self.selected_endpoint;
            if multi {
                spaces = spaces.child(self.rail_host(endpoint, selected, look, cx));
                rows[0] += 1;
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
            if !(multi && endpoint.collapsed) {
                for (index, indented, _) in
                    visible_workspace_entries(&snapshot.workspaces, collapsed_repos)
                {
                    let workspace = &snapshot.workspaces[index];
                    let focused = selected && workspace.focused;
                    if focused {
                        highlighted[0] = Some(rows[0]);
                    }
                    rows[0] += 1;
                    let icon = if indented {
                        RowIcon::None
                    } else {
                        self.avatars
                            .as_ref()
                            .filter(|_| endpoint_index == 0)
                            .and_then(|avatars| avatars.image(&workspace.new_workspace_cwd))
                            .map_or(RowIcon::Mark, RowIcon::Avatar)
                    };
                    spaces = spaces.child(self.rail_workspace(
                        &endpoint.id,
                        workspace,
                        (indented, focused),
                        icon,
                        look,
                        cx,
                    ));
                }
            }
            if !self.config.show_agents {
                continue;
            }
            let host =
                (multi && endpoint.id != crate::endpoint::LOCAL).then_some(endpoint.label.as_str());
            for agent in sorted_agents(&snapshot.agents, self.agent_sort) {
                let focused = selected && agent.focused;
                if focused {
                    highlighted[1] = Some(rows[1]);
                }
                rows[1] += 1;
                let (segments, name) = agent_labels(agent, snapshot, host);
                let mut hint = name.to_owned();
                for (segment, _) in segments.iter().filter(|(text, _)| *text != name) {
                    hint.push_str(" · ");
                    hint.push_str(segment);
                }
                let key = format!("rail-agent-{}-{}", endpoint.id, agent.pane_id);
                let id = agent.pane_id.clone();
                let navigate_endpoint = endpoint.id.clone();
                agents = agents.child(
                    rail_cell(&key, RAIL_DOT_CELL, focused, look, theme)
                        .child(status_dot(agent.agent_status, 1.25))
                        .tooltip(hint_tooltip(hint, theme))
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.navigate_endpoint(
                                &navigate_endpoint,
                                NavigationTarget::Pane(&id),
                                cx,
                            );
                            window.focus(&this.focus, cx);
                        })),
                );
            }
        }
        self.reveal_highlighted(highlighted);
        let foreground = theme.foreground;
        let active = theme.active;
        let icon_button = |id: &'static str, path: &'static str, hint: &'static str| {
            div()
                .id(id)
                .debug_selector(move || id.into())
                .size(px(28.))
                .flex_none()
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(crate::config::corners::CONTROL))
                .cursor_pointer()
                .hover(move |s| s.bg(rgb(active)))
                .tooltip(hint_tooltip(hint.into(), theme))
                .child(svg().path(path).size(px(16.)).text_color(rgb(foreground)))
        };
        div()
            .id("sidebar")
            .debug_selector(|| "sidebar".into())
            .relative()
            .w(px(RAIL_WIDTH))
            .flex_none()
            .h_full()
            .min_h_0()
            .overflow_hidden()
            .flex()
            .flex_col()
            .text_font(font)
            .text_size(px(font.size))
            .text_color(rgb(theme.foreground))
            .bg(rgb(theme.surface))
            .border_r_1()
            .border_color(rgb(theme.active))
            .child(
                div()
                    .debug_selector(|| "spaces-section".into())
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
                    .child(spaces)
                    .child(div().flex_none().flex().justify_center().py(px(4.)).child(
                        icon_button("new-workspace", "icons/plus.svg", "New Workspace").on_click(
                            cx.listener(|this, _, window, cx| {
                                this.command(Command::Workspace, window, cx)
                            }),
                        ),
                    )),
            )
            .when(self.config.show_agents, |rail| {
                rail.child(
                    div()
                        .debug_selector(|| "agents-section".into())
                        .flex()
                        .flex_col()
                        .flex_1()
                        .map(|mut section| {
                            section.style().flex_grow = Some(1. - split);
                            section
                        })
                        .min_h_0()
                        .overflow_hidden()
                        .border_t_1()
                        .border_color(rgb(theme.active))
                        .child(agents),
                )
            })
            .child(
                div()
                    .h(px(DEVICE_FOOTER_HEIGHT))
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_center()
                    .border_t_1()
                    .border_color(rgb(theme.active))
                    .child(
                        icon_button("device-settings", "icons/settings.svg", "Settings").on_click(
                            cx.listener(|this, _, window, cx| {
                                this.command(Command::Settings, window, cx);
                            }),
                        ),
                    ),
            )
            .child(self.resize_handle(RAIL_WIDTH, cx))
            .child(drag_capture(cx.entity().downgrade(), Drops::default()))
    }

    /// A device's divider: the devices icon, dimmed while the device is
    /// disabled, with the footer's green dot while it is connected. The name
    /// moves to the tooltip. Clicking it shows that device, as its full-width
    /// row does.
    fn rail_host(
        &self,
        endpoint: &Endpoint,
        selected: bool,
        look: SidebarLook,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let theme = &self.theme;
        let key = format!("rail-host-{}", endpoint.id);
        // The shown device's state lives on the window, as in its full row.
        let live = if selected { &self.live } else { &endpoint.live };
        let select_id = endpoint.id.clone();
        rail_cell(&key, RAIL_DOT_CELL + 6., selected, look, theme)
            .border_t_1()
            .border_color(rgb(theme.active))
            .child(
                svg()
                    .debug_selector({
                        let key = key.clone();
                        move || format!("icon-{key}")
                    })
                    .path("icons/devices.svg")
                    .size(px(14.))
                    .text_color(rgb(if endpoint.enabled {
                        theme.foreground
                    } else {
                        theme.muted
                    })),
            )
            .when(live.status.is_connected(), |cell| {
                cell.child(
                    div()
                        .debug_selector({
                            let key = key.clone();
                            move || format!("connected-{key}")
                        })
                        .absolute()
                        .top(px(MARK_INSET))
                        .right(px(MARK_INSET))
                        .size(px(5.))
                        .rounded_full()
                        .bg(rgb(CONNECTED)),
                )
            })
            .tooltip(hint_tooltip(
                format!("{} · {}", endpoint.label, endpoint.status()),
                theme,
            ))
            .on_click(cx.listener(move |this, _, window, cx| {
                this.select_endpoint(&select_id, cx);
                window.focus(&this.focus, cx);
            }))
    }

    /// A workspace's cell. The pull request number, when one is cached, is
    /// the most specific thing a narrow cell can say, so it replaces the
    /// avatar; worktree children fall back to a branch mark.
    fn rail_workspace(
        &self,
        endpoint_id: &str,
        workspace: &ClientShellWorkspace,
        (indented, focused): (bool, bool),
        icon: RowIcon,
        look: SidebarLook,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let theme = &self.theme;
        let font = &self.config.sidebar;
        let badge = workspace_badge(workspace, &self.menu.pr_cache, &self.git, theme);
        let key = format!("rail-{endpoint_id}-{}", workspace.workspace_id);
        let text_color = if focused {
            theme.foreground
        } else {
            theme.muted
        };
        let center = match (badge.as_ref().and_then(RowBadge::pr), icon, indented) {
            (Some(pr), _, _) => {
                // The `#` costs a digit's width the cell does not have.
                let number = pr.number().trim_start_matches('#');
                let glyphs = number.chars().count().max(1) as f32;
                let room = RAIL_WIDTH - 2. * (PR_MARGIN + PR_PADDING + 1.);
                // Shrink long numbers to fit rather than truncating digits.
                let size = PR_TEXT.min(font.size).min(room / (glyphs * 0.62)).floor();
                let color = pr.color();
                div()
                    .debug_selector({
                        let key = key.clone();
                        move || format!("pr-{key}")
                    })
                    .h(px(PR_HEIGHT))
                    .max_w(px(RAIL_WIDTH - 2. * PR_MARGIN))
                    .px(px(PR_PADDING))
                    .flex()
                    .items_center()
                    .rounded(px(crate::config::corners::SMALL))
                    .border_1()
                    .border_color(rgba((color << 8) | 0x80))
                    .bg(rgba((color << 8) | 0x24))
                    .text_size(px(size))
                    .line_height(px(PR_HEIGHT - 2.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(rgb(color))
                    .child(label_text(number))
                    .into_any_element()
            }
            (None, RowIcon::Avatar(image), _) => {
                let muted = theme.muted;
                img(image)
                    .size(px(20.))
                    .rounded_full()
                    .with_fallback(move || github_mark(muted).size(px(18.)).into_any_element())
                    .with_loading(move || github_mark(muted).size(px(18.)).into_any_element())
                    .into_any_element()
            }
            (None, _, true) => svg()
                .path("icons/git-branch.svg")
                .size(px(16.))
                .text_color(rgb(text_color))
                .into_any_element(),
            (None, _, false) => github_mark(text_color).size(px(18.)).into_any_element(),
        };
        let label = workspace_label(workspace, indented);
        let mut hint = label.to_owned();
        if let Some(branch) = workspace
            .branch
            .as_deref()
            .map(str::trim)
            .filter(|branch| !branch.is_empty() && !branch.ends_with(label))
        {
            hint.push_str(" · ");
            hint.push_str(branch);
        }
        if let Some(pr) = badge.as_ref().and_then(RowBadge::pr) {
            hint.push_str(" · ");
            hint.push_str(pr.number());
        }
        let navigate_endpoint = endpoint_id.to_owned();
        let context_endpoint = endpoint_id.to_owned();
        let id = workspace.workspace_id.clone();
        let context_id = id.clone();
        rail_cell(&key, RAIL_CELL, focused, look, theme)
            .child(center)
            // A worktree child keeps a short trunk on the left, so the rail
            // still reads as the tree the full sidebar draws.
            .when(indented, |cell| {
                cell.child(
                    div()
                        .absolute()
                        .left(px(look.inset() + 4.))
                        .top(px(RAIL_CELL * 0.25))
                        .h(px(RAIL_CELL * 0.5))
                        .w(px(1.))
                        .bg(rgb(theme.muted)),
                )
            })
            // Unknown means no agent reported, which a dot on every idle
            // workspace would only make harder to scan.
            .when(workspace.agent_status != AgentStatus::Unknown, |cell| {
                cell.child(
                    div()
                        .debug_selector({
                            let key = key.clone();
                            move || format!("status-{key}")
                        })
                        .absolute()
                        .top(px(MARK_INSET))
                        .right(px(MARK_INSET))
                        .child(status_dot(workspace.agent_status, 0.75)),
                )
            })
            .when(badge.as_ref().is_some_and(RowBadge::dirty), |cell| {
                cell.child(
                    crate::icons::uncommitted(theme, 10.)
                        .debug_selector({
                            let key = key.clone();
                            move || format!("dirty-{key}")
                        })
                        .absolute()
                        .bottom(px(MARK_INSET))
                        .right(px(MARK_INSET)),
                )
            })
            .tooltip(hint_tooltip(hint, theme))
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
                this.navigate_endpoint(&navigate_endpoint, NavigationTarget::Workspace(&id), cx);
                window.focus(&this.focus, cx);
            }))
    }
}

/// A clickable, full-width rail cell carrying the layout's highlight.
fn rail_cell(
    key: &str,
    height: f32,
    focused: bool,
    look: SidebarLook,
    theme: &Theme,
) -> Stateful<Div> {
    look.hover_group(
        div()
            .id(SharedString::from(key.to_owned()))
            .debug_selector({
                let key = key.to_owned();
                move || key
            })
            .relative()
            .h(px(height))
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .cursor_pointer()
            .child(look.highlight(key, focused, theme)),
    )
}

/// The same dot the full rows show, optionally larger where it is the whole
/// cell.
fn status_dot(status: AgentStatus, scale: f32) -> Div {
    let (diameter, filled, color) = status_style(status);
    div()
        .size(px(diameter * scale))
        .rounded_full()
        .border_1()
        .border_color(rgb(color))
        .when(filled, |dot| dot.bg(rgb(color)))
}

fn hint_tooltip(
    text: String,
    theme: &Theme,
) -> impl Fn(&mut Window, &mut App) -> AnyView + 'static {
    let text = SharedString::from(text);
    let (foreground, surface) = (theme.foreground, theme.surface);
    move |_, cx| {
        cx.new(|_| Hint {
            text: text.clone(),
            foreground,
            surface,
        })
        .into()
    }
}
