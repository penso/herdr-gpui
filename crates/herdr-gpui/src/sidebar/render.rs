//! Laying out the sidebar from kit sidebar parts: a header with the space
//! actions, the spaces and agents lists split by a resize handle, and the
//! device footer. Render works from prepared state and the bounded caches only.
//!
//! The lists are plain scroll containers of kit menu items rather than the
//! kit's `Sidebar`, which keeps a single list and its scroll state to itself:
//! the two lists scroll independently and follow the selection.

use super::{
    agents::{agent_labels, agent_row_label, agents_sort},
    panels,
    reorder::{self, Plan, WorkspaceDrag},
    row::{Fold, FoldTarget, RowSuffix, item, status_icon},
    sorted_agents, visible_workspace_entries,
    workspaces::{workspace_badge, workspace_label},
};
use crate::{Command, HerdrWindow, NavigationTarget, fonts::StyledFont};
use gpui_kit::component::{
    ActiveTheme, Icon, IconName, Sizable,
    button::{Button, ButtonVariants},
    h_flex,
    scroll::ScrollableElement,
    sidebar::{SidebarHeader, SidebarItem},
    v_flex,
};
use gpui_kit::{prelude::*, *};

impl HerdrWindow {
    pub(crate) fn render_sidebar(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let multi = self.endpoints.len() > 1;
        let size = self.config.sidebar.size;
        let border = cx.theme().sidebar_border;
        // A carried row is a raised card: opaque, so the rows it passes stay
        // hidden under it, and shadowed where the theme draws shadows.
        let card = (cx.theme().popover, cx.theme().radius, cx.theme().shadow);
        let mut spaces: Vec<AnyElement> = Vec::new();
        let mut agents: Vec<AnyElement> = Vec::new();
        // Child positions of the highlighted rows, for the one-time reveal below.
        let mut highlighted = [None; 2];
        // A lifted workspace row: each drop unit's rows in the spaces list, as
        // child positions with the shift they paint at, and the move every gap
        // makes, for the pointer handlers below.
        let mut drop_rows: Vec<(usize, usize, Pixels)> = Vec::new();
        let mut drop_requests = Vec::new();
        let mut drop_dragged = 0;
        let now = std::time::Instant::now();
        let mut sliding = false;
        for (endpoint_index, endpoint) in self.endpoints.iter().enumerate() {
            if !self.device_visible(&endpoint.id) {
                continue;
            }
            let selected = endpoint_index == self.selected_endpoint;
            let endpoint_id = endpoint.id.clone();
            if multi {
                let select_id = endpoint_id.clone();
                let online = if endpoint.live.status.is_connected() {
                    cx.theme().success
                } else {
                    cx.theme().muted_foreground
                };
                let host = item(
                    endpoint.label.clone(),
                    Some(
                        Icon::new(if endpoint_id == crate::endpoint::LOCAL {
                            IconName::HardDrive
                        } else {
                            IconName::Globe
                        })
                        .size_4()
                        .text_color(online),
                    ),
                    selected,
                    RowSuffix {
                        note: Some(endpoint.status().into()),
                        fold: Some(Fold {
                            id: format!("collapse-host-{endpoint_id}").into(),
                            collapsed: endpoint.collapsed,
                            view: cx.entity().downgrade(),
                            endpoint: endpoint_id.clone(),
                            target: FoldTarget::Host,
                        }),
                        ..Default::default()
                    },
                    size,
                )
                .disable(!endpoint.enabled);
                spaces.push(
                    div()
                        .id(SharedString::from(format!("host-{endpoint_id}")))
                        .debug_selector(|| format!("host-{endpoint_id}"))
                        .w_full()
                        .py_0p5()
                        .child(SidebarItem::render(
                            host,
                            SharedString::from(format!("host-item-{endpoint_id}")),
                            window,
                            cx,
                        ))
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.select_endpoint(&select_id, cx);
                            window.focus(&this.focus, cx);
                        }))
                        .into_any_element(),
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
            let entries = if multi && endpoint.collapsed {
                Vec::new()
            } else {
                visible_workspace_entries(&snapshot.workspaces, collapsed_repos)
            };
            let drag = self
                .workspace_drag
                .as_ref()
                .filter(|drag| selected && drag.previewing(&snapshot.workspaces));
            let floating = drag.is_some_and(WorkspaceDrag::floating);
            let plan = drag.and_then(|drag| Plan::new(&snapshot.workspaces, &drag.workspace));
            // Each unit's shift for the drop in preview, from the heights the
            // last frame laid out: shifting moves rows, never resizes them.
            let base = spaces.len();
            let shifts = plan.as_ref().map_or_else(Vec::new, |plan| {
                drop_requests = (0..=plan.len())
                    .map(|slot| plan.request(&snapshot.workspaces, slot))
                    .collect();
                drop_dragged = plan.dragged();
                let mut heights = vec![0.; plan.len()];
                for (position, &(index, _, _)) in entries.iter().enumerate() {
                    if let (Some(unit), Some(bounds)) = (
                        plan.unit_of(index),
                        self.sidebar_scroll[0].bounds_for_item(base + position),
                    ) {
                        heights[unit] += f32::from(bounds.size.height);
                    }
                }
                let slot = drag
                    .and_then(|drag| drag.target.as_ref())
                    .map_or(plan.dragged(), |target| target.slot);
                reorder::preview(&heights, plan.dragged(), slot)
            });
            for (index, indented, group) in entries {
                let workspace = &snapshot.workspaces[index];
                if selected && workspace.focused {
                    highlighted[0] = Some(spaces.len());
                }
                let unit = plan.as_ref().and_then(|plan| plan.unit_of(index));
                // The lifted row and the rows it carries, such as its group's
                // children, follow the pointer together while it floats.
                let carried = floating && unit == Some(drop_dragged);
                let shift = match (drag, unit) {
                    (Some(drag), Some(_)) if carried => {
                        drag.pin(&workspace.workspace_id, f32::from(drag.offset()), now);
                        drag.offset()
                    }
                    (Some(drag), Some(unit)) => {
                        let (at, moving) = drag.slide(&workspace.workspace_id, shifts[unit], now);
                        sliding |= moving;
                        px(at)
                    }
                    _ => px(0.),
                };
                if let Some(unit) = unit {
                    drop_rows.push((unit, spaces.len(), shift));
                }
                let id = workspace.workspace_id.clone();
                let press_id = id.clone();
                let context_id = id.clone();
                let hover_id = id.clone();
                let context_endpoint = endpoint_id.clone();
                let navigate_endpoint = endpoint_id.clone();
                let label = workspace_label(workspace, indented);
                let (pr, dirty) =
                    workspace_badge(workspace, &self.menu.pr_cache, &self.git, &self.theme);
                let removing = selected
                    && self.live.status.is_connected()
                    && self.removal.as_ref().is_some_and(|removal| {
                        removal.pending_for(
                            (self.selection_epoch, endpoint.generation),
                            &snapshot.boot_id,
                            &workspace.workspace_id,
                        )
                    });
                let avatar = (!indented)
                    .then_some(self.avatars.as_ref())
                    .flatten()
                    .filter(|_| endpoint_index == 0)
                    .and_then(|avatars| avatars.image(&workspace.new_workspace_cwd));
                let fold = group.map(|key| Fold {
                    id: format!("collapse-{endpoint_id}-{id}").into(),
                    collapsed: collapsed_repos.contains(&key),
                    view: cx.entity().downgrade(),
                    endpoint: endpoint_id.clone(),
                    target: FoldTarget::Repo(key),
                });
                let row = item(
                    label.to_owned(),
                    Some(status_icon(
                        if indented {
                            Icon::empty().path("icons/git-branch.svg")
                        } else {
                            Icon::empty().path("icons/github.svg")
                        },
                        workspace.agent_status,
                        cx,
                    )),
                    selected && workspace.focused,
                    RowSuffix {
                        removing,
                        dirty,
                        pr,
                        avatar,
                        fold,
                        ..Default::default()
                    },
                    size,
                );
                let row = div()
                    .id(SharedString::from(format!("workspace-{endpoint_id}-{id}")))
                    .debug_selector({
                        let key = if multi {
                            format!("workspace-{endpoint_id}-{id}")
                        } else {
                            format!("row-{label}")
                        };
                        move || key
                    })
                    .w_full()
                    .py_0p5()
                    // Worktrees hang off their repository the way the kit
                    // draws a submenu: indented behind a guide line.
                    .when(indented, |row| {
                        row.ml_3p5().pl_2p5().border_l_1().border_color(border)
                    })
                    .child(SidebarItem::render(
                        row,
                        SharedString::from(format!("workspace-item-{endpoint_id}-{id}")),
                        window,
                        cx,
                    ))
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
                    }))
                    // Only the selected endpoint's rows arm the hover menu:
                    // another endpoint's menu would have to select it first,
                    // and resting the pointer must not switch which daemon is
                    // shown. The feature is opt-in, so rows stay unarmed
                    // without it.
                    .when(selected && self.config.features.sidebar_hover_menu, |row| {
                        row.on_hover(cx.listener(move |this, hovered: &bool, window, _| {
                            this.hover_workspace(&hover_id, *hovered, window);
                        }))
                    })
                    // Holding a press lifts the row for reordering. Another
                    // endpoint's rows would have to select it first.
                    .when(selected, |row| {
                        row.on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, event: &MouseDownEvent, _, cx| {
                                if event.click_count == 1 {
                                    this.press_workspace(&press_id, event.position, cx);
                                }
                            }),
                        )
                    })
                    .when(shift != px(0.), |row| row.top(shift))
                    // The card occludes the rows it passes, so they stop
                    // answering hover while it is carried over them.
                    .when(carried, |row| {
                        let (background, radius, shadow) = card;
                        row.occlude()
                            .cursor_grabbing()
                            .rounded(radius)
                            .bg(background)
                            .when(shadow, |row| row.shadow_lg())
                    });
                spaces.push(if carried {
                    // Painted last so it floats over the rows it passes, while
                    // its layout slot keeps the others' positions stable.
                    deferred(row).with_priority(1).into_any_element()
                } else {
                    row.into_any_element()
                });
            }
            if !self.config.show_agents {
                continue;
            }
            for agent in sorted_agents(&snapshot.agents, self.agent_sort) {
                if selected && agent.focused {
                    highlighted[1] = Some(agents.len());
                }
                let id = agent.pane_id.clone();
                let navigate_endpoint = endpoint_id.clone();
                let host = (multi && endpoint_id != crate::endpoint::LOCAL)
                    .then_some(endpoint.label.as_str());
                let (segments, name) = agent_labels(agent, snapshot, host);
                let row = item(
                    agent_row_label(&segments, name),
                    Some(status_icon(
                        Icon::empty().path(
                            crate::icons::AgentIcon::from_identity(agent.agent.as_deref()).path(),
                        ),
                        agent.agent_status,
                        cx,
                    )),
                    selected && agent.focused,
                    RowSuffix::default(),
                    size,
                );
                agents.push(
                    div()
                        .id(SharedString::from(format!("agent-{endpoint_id}-{id}")))
                        .debug_selector({
                            let key = if multi {
                                format!("agent-{endpoint_id}-{id}")
                            } else {
                                format!("row-agent-{id}")
                            };
                            move || key
                        })
                        .w_full()
                        .py_0p5()
                        .child(SidebarItem::render(
                            row,
                            SharedString::from(format!("agent-item-{endpoint_id}-{id}")),
                            window,
                            cx,
                        ))
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.navigate_endpoint(
                                &navigate_endpoint,
                                NavigationTarget::Pane(&id),
                                cx,
                            );
                            window.focus(&this.focus, cx);
                        }))
                        .into_any_element(),
                );
            }
        }
        if sliding {
            window.request_animation_frame();
        }
        self.reveal(highlighted);
        let tracking = self.track_workspace_drag(drop_rows, drop_requests, drop_dragged, cx);
        if agents.is_empty() {
            agents.push(
                div()
                    .px_2()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child("No agents")
                    .into_any_element(),
            );
        }
        let spaces = list("spaces-scroll", &self.sidebar_scroll[0], spaces);
        let content = if self.config.show_agents {
            self.hold_split(window, cx);
            let agents = v_flex()
                .debug_selector(|| "agents-section".into())
                .size_full()
                .child(
                    section_heading("Agents", cx)
                        .justify_between()
                        .child(agents_sort(self, cx)),
                )
                .child(list("agents-scroll", &self.sidebar_scroll[1], agents));
            panels::split(
                &self.sidebar_panels.split,
                cx.entity().downgrade(),
                spaces,
                agents,
            )
            .into_any_element()
        } else {
            spaces.into_any_element()
        };
        let theme = cx.theme();
        v_flex()
            .id("sidebar")
            .debug_selector(|| "sidebar".into())
            .size_full()
            .min_h_0()
            .overflow_hidden()
            .text_font(&self.config.sidebar)
            .bg(theme.sidebar)
            .text_color(theme.sidebar_foreground)
            .child(self.render_sidebar_header(cx))
            .child(div().flex_1().min_h_0().child(content))
            .child(self.render_device_footer(cx))
            .child(tracking)
    }

    /// Follows a workspace drag through the whole window: capture-phase
    /// listeners keep tracking once the pointer leaves the list, and a lifted
    /// row's release is its drop, never a click on whatever lies under it.
    /// They read the gaps this frame laid out; every drag change notifies, so
    /// a moving drag always resolves against its own preview's frame.
    fn track_workspace_drag(
        &self,
        rows: Vec<(usize, usize, Pixels)>,
        requests: Vec<Option<reorder::MoveBlock>>,
        dragged: usize,
        cx: &mut Context<Self>,
    ) -> Canvas<()> {
        let view = cx.entity().downgrade();
        let scroll = self.sidebar_scroll[0].clone();
        canvas(
            |_, _, _| (),
            move |_, _, window, _| {
                let moving = view.clone();
                window.on_mouse_event(move |event: &MouseMoveEvent, phase, _, cx| {
                    if phase != DispatchPhase::Capture {
                        return;
                    }
                    let _ = moving.update(cx, |this, cx| {
                        if this.move_workspace_drag(
                            event.position,
                            event.pressed_button == Some(MouseButton::Left),
                            |lift| reorder::resolve(&rows, &requests, dragged, &scroll, lift),
                            cx,
                        ) {
                            cx.stop_propagation();
                        }
                    });
                });
                window.on_mouse_event(move |event: &MouseUpEvent, phase, _, cx| {
                    if phase != DispatchPhase::Capture || event.button != MouseButton::Left {
                        return;
                    }
                    let _ = view.update(cx, |this, cx| {
                        if this.release_workspace_drag(cx) {
                            cx.stop_propagation();
                        }
                    });
                });
            },
        )
        .absolute()
        .size_0()
    }

    fn render_sidebar_header(&self, cx: &mut Context<Self>) -> impl IntoElement {
        div().px_2().pt_2().child(
            SidebarHeader::new()
                .child(section_label("Spaces", cx))
                .child(
                    h_flex()
                        .gap_1()
                        .child(
                            Button::new("new-workspace")
                                .debug_selector(|| "new-workspace".into())
                                .ghost()
                                .xsmall()
                                .icon(IconName::Plus)
                                .tooltip("New workspace")
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.command(Command::Workspace, window, cx);
                                })),
                        )
                        .child(
                            Button::new("sidebar-menu")
                                .debug_selector(|| "sidebar-menu".into())
                                .ghost()
                                .xsmall()
                                .icon(IconName::Menu)
                                .tooltip("Menu")
                                .on_click(cx.listener(|this, event: &ClickEvent, window, cx| {
                                    this.menu.anchor = event.position();
                                    this.open_menu(window, cx);
                                })),
                        ),
                ),
        )
    }

    /// Follow the selection, but only once a frame has measured the viewport:
    /// the handle resolves the request against the previous frame's bounds, so
    /// an unmeasured list would scroll to a meaningless offset. Recording what
    /// was revealed keeps later frames from undoing the user's own scrolling.
    fn reveal(&self, highlighted: [Option<usize>; 2]) {
        for (list, row) in highlighted.iter().enumerate() {
            let Some(row) = *row else { continue };
            let scroll = &self.sidebar_scroll[list];
            if self.sidebar_revealed[list].get() == Some(row)
                || scroll.bounds().size.height <= px(0.)
            {
                continue;
            }
            let visible = scroll.bounds_for_item(row).is_some_and(|bounds| {
                let offset = scroll.offset().y;
                bounds.bottom() + offset > scroll.bounds().top()
                    && bounds.top() + offset < scroll.bounds().bottom()
            });
            // Even a partially visible worktree is already seen. GPUI's
            // reveal also moves clipped rows, so only request it off-screen.
            if !visible {
                scroll.scroll_to_item(row);
            }
            self.sidebar_revealed[list].set(Some(row));
        }
    }
}

/// One scrolling list of rows with the kit's scrollbar. Each row is a direct
/// child, so a row's position is its index for the reveal above.
fn list(id: &'static str, scroll: &ScrollHandle, rows: Vec<AnyElement>) -> Stateful<Div> {
    // Each list's frame has its own id, so the two scrollbars built from this
    // one call site keep separate state.
    div()
        .id(SharedString::from(format!("{id}-frame")))
        .relative()
        .size_full()
        .min_h_0()
        .child(
            div()
                .id(id)
                .debug_selector(move || id.into())
                .size_full()
                .overflow_y_scroll()
                .track_scroll(scroll)
                .px_2()
                .flex()
                .flex_col()
                .children(rows),
        )
        .vertical_scrollbar(scroll)
}

/// A section's caption, styled as the kit captions a sidebar group.
fn section_label(label: &'static str, cx: &App) -> Div {
    div()
        .debug_selector(move || format!("header-{label}"))
        .text_xs()
        .text_color(cx.theme().sidebar_foreground.opacity(0.7))
        .child(label)
}

fn section_heading(label: &'static str, cx: &App) -> Div {
    h_flex()
        .flex_none()
        .h_8()
        .px_4()
        .child(section_label(label, cx))
}
