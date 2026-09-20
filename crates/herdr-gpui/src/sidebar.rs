use super::{Command, HerdrWindow, NavigationTarget};
use crate::config::{FontConfig, Theme};
use gpui::{prelude::*, *};
use herdr_client::protocol::{AgentStatus, ClientShellAgent, ClientShellWorkspace};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, LazyLock};

const SIDEBAR_WIDTH: f32 = 232.;
const ROW_PADDING: f32 = 12.;
const STATUS_WIDTH: f32 = 5.;
pub(super) const LABEL_GAP: f32 = 8.;
const CHILD_INDENT: f32 = 16.;
pub(super) const ARROW_RESERVE: f32 = 18.;
pub(super) const HOST_ARROW_WIDTH: f32 = 12.;
pub(super) const HOST_GAP: f32 = 6.;
pub(super) const ICON_RESERVE: f32 = 18.;
pub(super) static GITHUB_ICON: LazyLock<Arc<Image>> = LazyLock::new(|| {
    Arc::new(Image::from_bytes(
        ImageFormat::Svg,
        include_bytes!("../../../assets/icons/github.svg").to_vec(),
    ))
});
#[cfg(any(test, feature = "integration-test"))]
pub(super) const LABEL_WIDTH: f32 =
    SIDEBAR_WIDTH - 1. - 2. * ROW_PADDING - STATUS_WIDTH - LABEL_GAP;

impl HerdrWindow {
    fn save_sidebar_width(&mut self) {
        self.sidebar_modified = true;
        if let Some(preferences) = &self.sidebar_preferences {
            preferences.save(self.sidebar_width);
        }
    }

    pub(super) fn render_sidebar(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let width = sidebar_width(self.sidebar_width, f32::from(window.viewport_size().width));
        // Hide secondary status in narrow windows, retaining useful host label space.
        let show_host_status = width >= 200.;
        let host_label_width = (width
            - 1.
            - 2. * ROW_PADDING
            - HOST_ARROW_WIDTH
            - HOST_GAP
            - if show_host_status { HOST_GAP + 67. } else { 0. })
        .max(0.);
        let view = cx.entity().downgrade();
        let font = &self.config.sidebar;
        let theme = &self.theme;
        let mut spaces = div()
            .id("spaces-scroll")
            .debug_selector(|| "spaces-scroll".into())
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .overflow_y_scroll();
        let mut agents = div()
            .id("agents-scroll")
            .debug_selector(|| "agents-scroll".into())
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .overflow_y_scroll();
        #[cfg(feature = "integration-test")]
        {
            spaces = spaces.track_scroll(&self.sidebar_scroll[0]);
            agents = agents.track_scroll(&self.sidebar_scroll[1]);
        }
        let multi = self.endpoints.len() > 1;
        let mut agent_count = 0;
        for (endpoint_index, endpoint) in self.endpoints.iter().enumerate() {
            let selected = endpoint_index == self.selected_endpoint;
            let endpoint_id = endpoint.id.clone();
            if multi {
                let collapse_id = endpoint_id.clone();
                let select_id = endpoint_id.clone();
                spaces = spaces.child(
                    div()
                        .id(SharedString::from(format!("host-{endpoint_id}")))
                        .debug_selector(|| format!("host-{endpoint_id}"))
                        .h(px(line_height(font) + 16.))
                        .flex_none()
                        .flex()
                        .items_center()
                        .gap(px(HOST_GAP))
                        .px(px(12.))
                        .when(selected, |row| row.bg(rgb(theme.active)))
                        .text_color(rgb(if endpoint.enabled {
                            theme.foreground
                        } else {
                            theme.muted
                        }))
                        .cursor_pointer()
                        .child(
                            div()
                                .id(SharedString::from(format!("collapse-host-{endpoint_id}")))
                                .w(px(HOST_ARROW_WIDTH))
                                .flex_none()
                                .child(label_text(if endpoint.collapsed {
                                    "\u{25b8}"
                                } else {
                                    "\u{25be}"
                                }))
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    cx.stop_propagation();
                                    if let Some(endpoint) =
                                        this.endpoints.iter_mut().find(|e| e.id == collapse_id)
                                    {
                                        endpoint.collapsed = !endpoint.collapsed;
                                    }
                                    cx.notify();
                                })),
                        )
                        .child(
                            div()
                                // As with workspace labels, avoid zero-basis text measurement.
                                .w(px(host_label_width))
                                .flex_none()
                                .overflow_hidden()
                                .child(
                                    div()
                                        .w(px(host_label_width))
                                        .truncate()
                                        .child(label_text(&endpoint.label)),
                                ),
                        )
                        .when(show_host_status, |row| {
                            row.child(
                                div()
                                    .w(px(67.))
                                    .flex_none()
                                    .text_right()
                                    .text_size(px(font.size * 0.75))
                                    .text_color(rgb(theme.muted))
                                    .child(endpoint.status()),
                            )
                        })
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.select_endpoint(&select_id, cx);
                            window.focus(&this.focus);
                        })),
                );
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
            for (index, indented, group) in
                visible_workspace_entries(&snapshot.workspaces, collapsed_repos)
            {
                if multi && endpoint.collapsed {
                    break;
                }
                let workspace = &snapshot.workspaces[index];
                let id = workspace.workspace_id.clone();
                let navigate_endpoint = endpoint_id.clone();
                let collapse_endpoint = endpoint_id.clone();
                spaces = spaces.child(
                    row(
                        workspace_label(workspace, indented),
                        first_text([workspace.branch.as_deref()], ""),
                        workspace.agent_status,
                        selected && workspace.focused,
                        indented,
                        group.is_some() || indented,
                        width,
                        (!indented).then(|| {
                            self.avatars
                                .as_ref()
                                .filter(|_| endpoint_index == 0)
                                .and_then(|avatars| avatars.image(&workspace.new_workspace_cwd))
                                .unwrap_or_else(|| GITHUB_ICON.clone())
                        }),
                        (font, theme),
                    )
                    .when_some(group, |row, key| {
                        let collapsed = collapsed_repos.contains(&key);
                        row.child(
                            div()
                                .id(SharedString::from(format!("collapse-{endpoint_id}-{id}")))
                                .debug_selector(move || format!("collapse-{index}"))
                                .w(px(ARROW_RESERVE - LABEL_GAP))
                                .h(px(2. * line_height(font)))
                                .flex_none()
                                .text_size(px(16.))
                                .text_color(rgb(theme.muted))
                                .hover(|s| s.text_color(rgb(theme.foreground)))
                                .child(label_text(if collapsed { "\u{25b8}" } else { "\u{25be}" }))
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    cx.stop_propagation();
                                    let collapsed = if collapse_endpoint == super::endpoint::LOCAL {
                                        &mut this.collapsed_repos
                                    } else if let Some(endpoint) = this
                                        .endpoints
                                        .iter_mut()
                                        .find(|e| e.id == collapse_endpoint)
                                    {
                                        &mut endpoint.collapsed_repos
                                    } else {
                                        return;
                                    };
                                    if !collapsed.remove(&key) {
                                        collapsed.insert(key.clone());
                                    }
                                    cx.notify();
                                })),
                        )
                    })
                    .id(SharedString::from(format!("workspace-{endpoint_id}-{id}")))
                    .when(multi, |row| {
                        row.debug_selector(|| format!("workspace-{endpoint_id}-{id}"))
                    })
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.navigate_endpoint(
                            &navigate_endpoint,
                            NavigationTarget::Workspace(&id),
                            cx,
                        );
                        window.focus(&this.focus);
                    })),
                );
            }
            for agent in &snapshot.agents {
                agent_count += 1;
                let id = agent.pane_id.clone();
                let navigate_endpoint = endpoint_id.clone();
                let (name, kind) = agent_labels(agent);
                let detail = if multi {
                    format!("{} / {kind}", endpoint.label)
                } else {
                    kind.to_owned()
                };
                agents = agents.child(
                    row(
                        name,
                        &detail,
                        agent.agent_status,
                        selected && agent.focused,
                        false,
                        false,
                        width,
                        None,
                        (font, theme),
                    )
                    .id(SharedString::from(format!("agent-{endpoint_id}-{id}")))
                    .when(multi, |row| {
                        row.debug_selector(|| format!("agent-{endpoint_id}-{id}"))
                    })
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.navigate_endpoint(&navigate_endpoint, NavigationTarget::Pane(&id), cx);
                        window.focus(&this.focus);
                    })),
                );
            }
        }
        if agent_count == 0 {
            agents = agents.child(
                div()
                    .px(px(12.))
                    .text_color(rgb(theme.muted))
                    .truncate()
                    .child("no agents"),
            );
        }
        div()
            .id("sidebar")
            .debug_selector(|| "sidebar".into())
            .relative()
            .w(px(width))
            .flex_none()
            .h_full()
            .min_h_0()
            .overflow_hidden()
            .flex()
            .flex_col()
            .font_family(font.family.clone())
            .text_size(px(font.size))
            .line_height(px(line_height(font)))
            .text_color(rgb(theme.foreground))
            .bg(rgb(theme.surface))
            .border_r_1()
            .border_color(rgb(theme.active))
            // Zero flex bases keep long workspace lists from displacing agents.
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_h_0()
                    .overflow_hidden()
                    .child(header("spaces", font, theme))
                    .child(spaces)
                    .child(
                        div()
                            .flex_none()
                            .h(px(line_height(font) + 10.))
                            .px(px(12.))
                            .flex()
                            .items_center()
                            .text_color(rgb(theme.muted))
                            .gap(px(20.))
                            .child(
                                div()
                                    .id("new-workspace")
                                    .cursor_pointer()
                                    .hover(|s| s.text_color(rgb(theme.foreground)))
                                    .child("new")
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.command(Command::Workspace, window, cx)
                                    })),
                            )
                            .child(
                                div()
                                    .id("sidebar-menu")
                                    .debug_selector(|| "sidebar-menu".into())
                                    .cursor_pointer()
                                    .hover(|s| s.text_color(rgb(theme.foreground)))
                                    .child(label_text("menu"))
                                    .on_click(cx.listener(
                                        |this, event: &ClickEvent, window, cx| {
                                            this.menu.anchor = event.position();
                                            this.open_menu(window, cx);
                                        },
                                    )),
                            ),
                    ),
            )
            .child(div().h(px(1.)).flex_none().bg(rgb(theme.active)))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_h_0()
                    .overflow_hidden()
                    .child(header("agents", font, theme))
                    .child(agents),
            )
            .child(
                div()
                    .id("sidebar-resize")
                    .debug_selector(|| "sidebar-resize".into())
                    .absolute()
                    .right_0()
                    .top_0()
                    .h_full()
                    .w(px(6.))
                    .cursor(CursorStyle::ResizeLeftRight)
                    .hover(|s| s.bg(rgba(0x78a9ff44)))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, event: &MouseDownEvent, _, cx| {
                            cx.stop_propagation();
                            this.sidebar_modified = true;
                            if event.click_count == 2 {
                                this.sidebar_drag = None;
                                this.sidebar_width = None;
                                this.save_sidebar_width();
                            } else {
                                this.sidebar_drag = Some((f32::from(event.position.x), width));
                            }
                            cx.notify();
                        }),
                    ),
            )
            .child(
                canvas(
                    |_, _, _| (),
                    move |_, _, window, _| {
                        // Capture globally so dragging continues outside the narrow divider,
                        // and terminal handlers never receive the resize gesture's release.
                        let moving = view.clone();
                        window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, cx| {
                            if phase == DispatchPhase::Capture {
                                let _ = moving.update(cx, |this, cx| {
                                    if let Some((start, width)) = this.sidebar_drag {
                                        this.sidebar_width = Some(sidebar_width(
                                            Some(width + f32::from(event.position.x) - start),
                                            f32::from(window.viewport_size().width),
                                        ));
                                        cx.stop_propagation();
                                        cx.notify();
                                    }
                                });
                            }
                        });
                        let released = view.clone();
                        window.on_mouse_event(move |event: &MouseUpEvent, phase, _, cx| {
                            if phase == DispatchPhase::Capture && event.button == MouseButton::Left
                            {
                                let _ = released.update(cx, |this, cx| {
                                    if this.sidebar_drag.take().is_some() {
                                        this.save_sidebar_width();
                                        cx.stop_propagation();
                                        cx.notify();
                                    }
                                });
                            }
                        });
                    },
                )
                .absolute()
                .size_full(),
            )
    }
}

// Preserve the sidebar's compact 12px font / 16px line defaults as fonts scale.
fn line_height(font: &FontConfig) -> f32 {
    font.size * 4. / 3.
}

fn sidebar_width(preferred: Option<f32>, window_width: f32) -> f32 {
    // Keep useful label space and reserve at least 240 logical pixels for the terminal.
    preferred
        .unwrap_or(SIDEBAR_WIDTH)
        .clamp(160., 480.)
        .min((window_width - 240.).max(0.))
}

fn header(label: &'static str, font: &FontConfig, theme: &Theme) -> Div {
    div()
        .flex_none()
        .h(px(line_height(font) + 12.))
        .px(px(12.))
        .flex()
        .items_center()
        .text_size(px(font.size * 5. / 6.))
        .text_color(rgb(theme.muted))
        .child(label)
}

#[allow(clippy::too_many_arguments)]
fn row(
    name: &str,
    detail: &str,
    status: AgentStatus,
    focused: bool,
    indented: bool,
    reserve_arrow: bool,
    width: f32,
    workspace_icon: Option<Arc<Image>>,
    appearance: (&FontConfig, &Theme),
) -> Div {
    let (font, theme) = appearance;
    let icon_reserve = if workspace_icon.is_some() {
        ICON_RESERVE
    } else {
        0.
    };
    let indent = if indented { CHILD_INDENT } else { 0. };
    let label_width = (width
        - 1.
        - 2. * ROW_PADDING
        - STATUS_WIDTH
        - LABEL_GAP
        - indent
        - if reserve_arrow { ARROW_RESERVE } else { 0. })
    .max(0.);
    div()
        .debug_selector(|| format!("row-{name}"))
        .h(px(2. * line_height(font) + 8.))
        .w_full()
        .min_w_0()
        .flex_none()
        .pl(px(ROW_PADDING + indent))
        .pr(px(ROW_PADDING))
        .flex()
        .items_start()
        .gap(px(LABEL_GAP))
        .py(px(4.))
        .cursor_pointer()
        .when(focused, |s| s.bg(rgb(theme.active)))
        .hover(|s| s.bg(rgb(theme.active)))
        .child(status_indicator(status, font, theme))
        .child(
            div()
                .flex()
                .flex_col()
                // Avoid zero-basis measurement: GPUI 0.2.2 mutates text run
                // lengths when truncating and reuses them on wider measurements.
                .w(px(label_width))
                .flex_none()
                .overflow_hidden()
                .debug_selector(|| format!("column-{name}"))
                .child(
                    div()
                        .relative()
                        .w(px(label_width))
                        .h(px(line_height(font)))
                        .when_some(workspace_icon, |title, image| {
                            title.child(
                                div()
                                    .debug_selector(|| format!("github-{name}"))
                                    .absolute()
                                    .left_0()
                                    .top(px((line_height(font) - 12.) / 2.))
                                    .size(px(12.))
                                    .flex_none()
                                    .overflow_hidden()
                                    .child(
                                        img(image)
                                            .size_full()
                                            .rounded_full()
                                            .with_fallback(|| {
                                                img(GITHUB_ICON.clone())
                                                    .size_full()
                                                    .rounded_full()
                                                    .into_any_element()
                                            })
                                            .with_loading(|| {
                                                img(GITHUB_ICON.clone())
                                                    .size_full()
                                                    .rounded_full()
                                                    .into_any_element()
                                            }),
                                    ),
                            )
                        })
                        .child(
                            div()
                                .debug_selector(|| format!("name-{name}"))
                                .ml(px(icon_reserve))
                                .w(px((label_width - icon_reserve).max(0.)))
                                .flex_none()
                                .truncate()
                                .child(label_text(name)),
                        ),
                )
                .child(
                    div()
                        .debug_selector(|| format!("detail-{name}"))
                        .w(px(label_width))
                        .truncate()
                        .text_color(rgb(theme.muted))
                        .child(label_text(detail)),
                ),
        )
}

#[cfg(any(test, feature = "integration-test"))]
pub(crate) mod layout_tests;

#[cfg(all(feature = "integration-test", target_os = "macos"))]
pub(crate) mod native_tests;

#[cfg(not(any(test, feature = "integration-test")))]
fn label_text(text: &str) -> SharedString {
    text.to_owned().into()
}

#[cfg(any(test, feature = "integration-test"))]
fn label_text(text: &str) -> layout_tests::ProbeText {
    layout_tests::ProbeText(text.to_owned().into())
}

fn first_text<'a>(values: impl IntoIterator<Item = Option<&'a str>>, fallback: &'a str) -> &'a str {
    values
        .into_iter()
        .flatten()
        .map(str::trim)
        .find(|s| !s.is_empty())
        .unwrap_or(fallback)
}

fn agent_labels(agent: &ClientShellAgent) -> (&str, &str) {
    let kind = first_text(
        [agent.display_agent.as_deref(), agent.agent.as_deref()],
        "agent",
    );
    let name = first_text(
        [
            agent.name.as_deref(),
            agent.title.as_deref(),
            agent.terminal_title_stripped.as_deref(),
        ],
        kind,
    );
    (name, kind)
}

// Match the expanded upstream shell order, including orphaned linked worktrees.
fn workspace_entries(workspaces: &[ClientShellWorkspace]) -> Vec<(usize, bool)> {
    let mut groups = HashMap::<&str, (Option<usize>, Vec<usize>)>::new();
    for (index, workspace) in workspaces.iter().enumerate() {
        if let Some(worktree) = &workspace.worktree {
            let (parent, members) = groups.entry(&worktree.key).or_default();
            if !worktree.is_linked_worktree && parent.is_none() {
                *parent = Some(index);
            }
            members.push(index);
        }
    }
    let mut emitted = HashSet::new();
    let mut entries = Vec::with_capacity(workspaces.len());
    for (index, workspace) in workspaces.iter().enumerate() {
        let group = workspace.worktree.as_ref().and_then(|tree| {
            let (parent, members) = groups.get(tree.key.as_str())?;
            Some((tree.key.as_str(), (*parent)?, members))
        });
        if let Some((key, parent, members)) = group {
            if emitted.insert(key) {
                entries.push((parent, false));
                entries.extend(members.iter().filter(|&&i| i != parent).map(|&i| (i, true)));
            }
        } else {
            entries.push((index, false));
        }
    }
    entries
}

fn visible_workspace_entries(
    workspaces: &[ClientShellWorkspace],
    collapsed: &HashSet<String>,
) -> Vec<(usize, bool, Option<String>)> {
    let entries = workspace_entries(workspaces);
    entries
        .iter()
        .enumerate()
        .filter_map(|(position, &(index, child))| {
            let key = workspaces[index].worktree.as_ref().map(|tree| &tree.key);
            if child && key.is_some_and(|key| collapsed.contains(key)) {
                return None;
            }
            let group = (!child && entries.get(position + 1).is_some_and(|entry| entry.1))
                .then(|| key.cloned())
                .flatten();
            Some((index, child, group))
        })
        .collect()
}

fn workspace_label(workspace: &ClientShellWorkspace, indented: bool) -> &str {
    let branch = (indented && !workspace.custom_label)
        .then_some(workspace.branch.as_deref())
        .flatten()
        .map(|branch| branch.strip_prefix("worktree/").unwrap_or(branch));
    first_text([branch, Some(&workspace.label)], "workspace")
}

fn status_indicator(status: AgentStatus, font: &FontConfig, theme: &Theme) -> Div {
    // Upstream dots: working/blocked/done filled, idle hollow, unknown a small dot.
    let (diameter, filled, color) = status_style(status, theme);
    div()
        .size(px(STATUS_WIDTH))
        .mt(px((line_height(font) - STATUS_WIDTH) / 2.))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .child(
            div()
                .size(px(diameter))
                .rounded_full()
                .border_1()
                .border_color(rgb(color))
                .when(filled, |dot| dot.bg(rgb(color))),
        )
}

fn status_style(status: AgentStatus, theme: &Theme) -> (f32, bool, u32) {
    match status {
        AgentStatus::Working => (STATUS_WIDTH, true, theme.palette[3]),
        AgentStatus::Blocked => (STATUS_WIDTH, true, theme.palette[1]),
        AgentStatus::Done => (STATUS_WIDTH, true, theme.palette[6]),
        AgentStatus::Idle => (STATUS_WIDTH, false, theme.palette[2]),
        AgentStatus::Unknown => (2., true, theme.muted),
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::{
        AgentStatus, ClientShellAgent, ClientShellWorkspace, STATUS_WIDTH, agent_labels,
        first_text, layout_tests, status_style, workspace_entries, workspace_label,
    };

    #[test]
    fn hierarchy_uses_git_metadata_and_emits_each_workspace_once() {
        let mut workspaces = layout_tests::snapshot(7).workspaces;
        for workspace in &mut workspaces {
            workspace.worktree = None;
            workspace.label = "same label".into();
            workspace.branch = Some("main".into());
        }
        for (index, key, linked) in [
            (0, "/repo/.git", true),
            (2, "/repo/.git", false),
            (3, "/orphan/.git", true),
            (4, "/repo/.git", true),
            (5, "/other/.git", false),
            (6, "/orphan/.git", true),
        ] {
            workspaces[index].worktree = Some(herdr_client::protocol::ClientShellWorktree {
                key: key.into(),
                label: "same repo name".into(),
                is_linked_worktree: linked,
            });
        }
        workspaces[2].branch = Some("develop".into());
        assert_eq!(
            workspace_entries(&workspaces),
            vec![
                (2, false),
                (0, true),
                (4, true),
                (1, false),
                (3, false),
                (5, false),
                (6, false),
            ]
        );
        workspaces[2].worktree = None;
        assert_eq!(
            workspace_entries(&workspaces),
            (0..7).map(|i| (i, false)).collect::<Vec<_>>()
        );
        assert!(workspace_entries(&[]).is_empty());
    }

    #[test]
    fn collapse_uses_repository_identity_without_mutating_selection() {
        let mut workspaces = layout_tests::snapshot(7).workspaces;
        workspaces[4].focused = true;
        let before = workspaces.clone();
        let collapsed = std::collections::HashSet::from(["/fixture/agent-launcher/.git".into()]);
        let entries = super::visible_workspace_entries(&workspaces, &collapsed);
        assert_eq!(
            entries.iter().map(|entry| entry.0).collect::<Vec<_>>(),
            vec![0, 1, 2, 3, 6]
        );
        assert_eq!(entries.iter().filter(|entry| entry.2.is_some()).count(), 1);
        assert_eq!(workspaces, before);
        workspaces[3].label = "renamed".into();
        assert_eq!(
            super::visible_workspace_entries(&workspaces, &collapsed).len(),
            5
        );
        workspaces.remove(5);
        workspaces.remove(4);
        assert!(
            super::visible_workspace_entries(&workspaces, &collapsed)
                .iter()
                .all(|entry| entry.2.is_none())
        );
        workspaces.remove(3);
        assert_eq!(
            super::visible_workspace_entries(&workspaces, &collapsed).len(),
            4
        );
    }

    #[test]
    fn child_labels_follow_upstream_custom_label_and_branch_rules() {
        let mut workspace = layout_tests::snapshot(1).workspaces.remove(0);
        workspace.branch = Some("worktree/fix-sidebar".into());
        assert_eq!(workspace_label(&workspace, true), "fix-sidebar");
        assert_eq!(workspace_label(&workspace, false), "herdr");
        workspace.custom_label = true;
        assert_eq!(workspace_label(&workspace, true), "herdr");
        workspace.custom_label = false;
        workspace.branch = None;
        assert_eq!(workspace_label(&workspace, true), "herdr");
    }

    #[test]
    fn text_fallback_skips_missing_and_blank_metadata() {
        assert_eq!(first_text([None, Some(" \t"), Some(" main ")], ""), "main");
        assert_eq!(first_text([None, Some("")], ""), "");
        assert_eq!(first_text([Some(" ")], "workspace"), "workspace");
    }

    #[test]
    fn agent_name_title_and_kind_fallbacks() {
        let mut agent = ClientShellAgent {
            pane_id: "p".into(),
            workspace_id: "w".into(),
            tab_id: "t".into(),
            name: Some("review".into()),
            title: Some("Fix sidebar".into()),
            display_agent: Some("Claude Code".into()),
            agent: Some("claude".into()),
            terminal_title: Some("raw title".into()),
            terminal_title_stripped: Some("terminal".into()),
            agent_status: AgentStatus::Unknown,
            state_change_seq: 0,
            state_labels: vec![],
            tokens: vec![],
            focused: false,
        };
        assert_eq!(agent_labels(&agent), ("review", "Claude Code"));
        agent.name = Some(" ".into());
        assert_eq!(agent_labels(&agent), ("Fix sidebar", "Claude Code"));
        agent.title = None;
        agent.display_agent = None;
        assert_eq!(agent_labels(&agent), ("terminal", "claude"));
        agent.terminal_title_stripped = None;
        assert_eq!(agent_labels(&agent), ("claude", "claude"));
        agent.agent = None;
        assert_eq!(agent_labels(&agent), ("agent", "agent"));
    }

    #[test]
    fn status_colors_follow_the_supplied_theme() {
        let mut theme = crate::config::Theme::default();
        theme.palette[1] = 0x112233;
        theme.palette[2] = 0x223344;
        theme.palette[3] = 0x334455;
        theme.palette[6] = 0x667788;
        theme.muted = 0x778899;
        for (status, color) in [
            (AgentStatus::Blocked, 0x112233),
            (AgentStatus::Idle, 0x223344),
            (AgentStatus::Working, 0x334455),
            (AgentStatus::Done, 0x667788),
            (AgentStatus::Unknown, 0x778899),
        ] {
            assert_eq!(status_style(status, &theme).2, color);
        }
    }

    #[test]
    fn status_shapes_match_upstream_dots_and_wire_casing() {
        let snapshot = layout_tests::snapshot(1);
        for (wire, status) in [
            ("idle", AgentStatus::Idle),
            ("working", AgentStatus::Working),
            ("blocked", AgentStatus::Blocked),
            ("done", AgentStatus::Done),
            ("unknown", AgentStatus::Unknown),
        ] {
            let mut value = serde_json::to_value(&snapshot.workspaces[0]).unwrap();
            value["agent_status"] = wire.into();
            let workspace: ClientShellWorkspace = serde_json::from_value(value).unwrap();
            assert_eq!(workspace.agent_status, status);
            let mut value = serde_json::to_value(&snapshot.agents[0]).unwrap();
            value["agent_status"] = wire.into();
            let agent: ClientShellAgent = serde_json::from_value(value).unwrap();
            assert_eq!(agent.agent_status, status);
            assert_eq!(serde_json::to_value(status).unwrap(), wire);
            let theme = crate::config::Theme::default();
            let (diameter, filled, color) = status_style(status, &theme);
            assert_eq!(
                color,
                match status {
                    AgentStatus::Working => theme.palette[3],
                    AgentStatus::Blocked => theme.palette[1],
                    AgentStatus::Done => theme.palette[6],
                    AgentStatus::Idle => theme.palette[2],
                    AgentStatus::Unknown => theme.muted,
                }
            );
            assert_eq!(filled, status != AgentStatus::Idle);
            assert_eq!(
                diameter,
                if status == AgentStatus::Unknown {
                    2.
                } else {
                    STATUS_WIDTH
                }
            );
        }
    }
}
