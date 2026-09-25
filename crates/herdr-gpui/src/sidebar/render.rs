//! Laying out the sidebar: the two lists, their headings, and the drag handle
//! that resizes the panel. Render works from prepared state and the bounded
//! caches only.

use super::{
    ARROW_RESERVE, DEVICE_FOOTER_HEIGHT, HOST_ARROW_WIDTH, HOST_GAP, SidebarDrag,
    agents::agent_labels,
    agents_sort, label_text,
    layout::{self, SidebarLook},
    line_height,
    metrics::COLLAPSE_BELOW,
    reorder::{self, Plan},
    row::first_text,
    row::{RowIcon, RowKind, RowLift, RowTree, row},
    sidebar_width, sorted_agents, visible_workspace_entries,
    workspaces::{workspace_badge, workspace_label},
};
use crate::{
    Command, HerdrWindow, NavigationTarget,
    config::{FontConfig, Theme},
    fonts::StyledFont,
    preferences::SidebarMode,
};
use gpui::{prelude::*, *};

impl HerdrWindow {
    pub(crate) fn render_sidebar(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        if self.sidebar_mode == SidebarMode::Collapsed {
            return self.render_rail(cx);
        }
        let width = sidebar_width(self.sidebar_width, f32::from(window.viewport_size().width));
        let split = self.sidebar_split.unwrap_or(0.5).clamp(0.1, 0.9);
        let look = layout::for_mode(self.config.layout.mode);
        let layout = look.density;
        let content_x = look.content_x();
        let gap = layout.gap();
        // Hide secondary status in narrow windows, retaining useful host label space.
        let show_host_status = width >= 200.;
        let host_label_width = (look.content_width(width)
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
        spaces = spaces.track_scroll(&self.sidebar_scroll[0]);
        agents = agents.track_scroll(&self.sidebar_scroll[1]);
        let multi = self.endpoints.len() > 1;
        let mut agent_count = 0;
        // Child positions of the highlighted rows, for the one-time reveal below.
        // Agent rows are counted by `agent_count`, which indexes that list.
        let mut space_rows = 0usize;
        let mut highlighted = [None; 2];
        // A lifted workspace row: each drop unit's rows in the spaces list, and
        // the move every gap makes, for the pointer handler below.
        let mut drop_rows = Vec::new();
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
                let collapse_id = endpoint_id.clone();
                let select_id = endpoint_id.clone();
                spaces = spaces.child(
                    div()
                        .id(SharedString::from(format!("host-{endpoint_id}")))
                        .debug_selector(|| format!("host-{endpoint_id}"))
                        .h(px(line_height(font)
                            + 2. * layout.host_padding()
                            + look.chrome_height()))
                        .flex_none()
                        .relative()
                        .flex()
                        .items_center()
                        .gap(px(HOST_GAP))
                        .px(px(content_x))
                        // Hosts mark selection only; they do not join the rows'
                        // hover group.
                        .child(look.highlight(&format!("host-{endpoint_id}"), selected, theme))
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
                            window.focus(&this.focus, cx);
                        })),
                );
                space_rows += 1;
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
            let entries = visible_workspace_entries(&snapshot.workspaces, collapsed_repos);
            let drag = self
                .workspace_drag
                .as_ref()
                .filter(|drag| selected && drag.previewing(&snapshot.workspaces));
            let floating = drag.is_some_and(|drag| drag.floating());
            let plan = drag.and_then(|drag| Plan::new(&snapshot.workspaces, &drag.workspace));
            // Each unit's shift for the drop in preview, from the heights the
            // last frame laid out: shifting moves rows, never resizes them.
            let base = space_rows;
            let shifts = plan.as_ref().map_or_else(Vec::new, |plan| {
                drop_requests = (0..=plan.len())
                    .map(|slot| plan.request(&snapshot.workspaces, slot))
                    .collect();
                drop_dragged = plan.dragged();
                let mut heights = vec![0.; plan.len()];
                for (position, entry) in entries.iter().enumerate() {
                    if let (Some(unit), Some(bounds)) = (
                        plan.unit_of(entry.0),
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
            // A child closes the group when no child follows it.
            let closes: Vec<bool> = (0..entries.len())
                .map(|position| {
                    entries[position].1 && !entries.get(position + 1).is_some_and(|next| next.1)
                })
                .collect();
            for (position, (index, indented, group)) in entries.into_iter().enumerate() {
                if multi && endpoint.collapsed {
                    break;
                }
                let workspace = &snapshot.workspaces[index];
                if selected && workspace.focused {
                    highlighted[0] = Some(space_rows);
                }
                let unit = plan.as_ref().and_then(|plan| plan.unit_of(index));
                // The lifted row and the rows it carries, such as its group's
                // children, follow the pointer together while it floats.
                let carried = unit.is_some_and(|unit| unit == drop_dragged) && floating;
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
                    drop_rows.push((unit, space_rows, shift));
                }
                space_rows += 1;
                let id = workspace.workspace_id.clone();
                let press_id = id.clone();
                let context_id = id.clone();
                let hover_id = id.clone();
                let context_endpoint = endpoint_id.clone();
                let navigate_endpoint = endpoint_id.clone();
                let collapse_endpoint = endpoint_id.clone();
                let reserve_arrow = group.is_some() || indented;
                let tree = match (indented, closes[position]) {
                    (false, _) => RowTree::None,
                    (true, false) => RowTree::Child,
                    (true, true) => RowTree::LastChild,
                };
                let arrow = group.map(|key| {
                    let collapsed = collapsed_repos.contains(&key);
                    div()
                        .id(SharedString::from(format!("collapse-{endpoint_id}-{id}")))
                        .debug_selector(move || format!("collapse-{index}"))
                        .w(px(ARROW_RESERVE - gap))
                        .h(px(
                            line_height(font) * if layout.workspace_details() { 2. } else { 1. }
                        ))
                        .flex_none()
                        .text_size(px(16.))
                        .text_color(rgb(theme.muted))
                        .hover(|s| s.text_color(rgb(theme.foreground)))
                        .child(label_text(if collapsed { "\u{25b8}" } else { "\u{25be}" }))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            cx.stop_propagation();
                            let collapsed = if collapse_endpoint == crate::endpoint::LOCAL {
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
                        }))
                });
                let label = workspace_label(workspace, indented);
                let element = row(
                    label,
                    &[(label, true)],
                    first_text([workspace.branch.as_deref()], ""),
                    RowKind::Workspace,
                    workspace.agent_status,
                    selected
                        && self.live.status.is_connected()
                        && self.removal.as_ref().is_some_and(|removal| {
                            removal.pending_for(
                                (self.selection_epoch, endpoint.generation),
                                &snapshot.boot_id,
                                &workspace.workspace_id,
                            )
                        }),
                    selected && workspace.focused,
                    match (carried, floating) {
                        (true, _) => RowLift::Lifted,
                        (false, true) => RowLift::Passed,
                        (false, false) => RowLift::Resting,
                    },
                    tree,
                    reserve_arrow,
                    width,
                    if indented {
                        RowIcon::None
                    } else {
                        self.avatars
                            .as_ref()
                            .filter(|_| endpoint_index == 0)
                            .and_then(|avatars| avatars.image(&workspace.new_workspace_cwd))
                            .map_or(RowIcon::Mark, RowIcon::Avatar)
                    },
                    arrow,
                    workspace_badge(workspace, &self.menu.pr_cache, &self.git, theme),
                    look,
                    (font, theme),
                )
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
                .when(
                    self.menu.page == Some(crate::menu::Page::Workspace),
                    |row| {
                        let view = cx.entity().downgrade();
                        let endpoint = endpoint_id.clone();
                        let workspace = id.clone();
                        row.child(
                            canvas(
                                |_, _, _| (),
                                move |bounds, _, window, _| {
                                    let bounds = bounds.intersect(&window.content_mask().bounds);
                                    window.on_mouse_event(
                                        move |event: &MouseDownEvent, phase, window, cx| {
                                            // The overlay dismisses first in bubble order. Use
                                            // clipped row geometry because it occludes our hitbox.
                                            if phase == DispatchPhase::Bubble
                                                && event.button == MouseButton::Right
                                                && bounds.contains(&event.position)
                                            {
                                                let _ = view.update(cx, |this, cx| {
                                                    cx.stop_propagation();
                                                    if this.navigate_endpoint(
                                                        &endpoint,
                                                        NavigationTarget::Workspace(&workspace),
                                                        cx,
                                                    ) {
                                                        this.open_workspace_menu(
                                                            &workspace,
                                                            event.position,
                                                            window,
                                                            cx,
                                                        );
                                                        this.menu.opening_right_click = this
                                                            .menu
                                                            .page
                                                            == Some(crate::menu::Page::Workspace);
                                                    }
                                                });
                                            }
                                        },
                                    );
                                },
                            )
                            .absolute()
                            .inset_0()
                            .size_full(),
                        )
                    },
                )
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
                    window.focus(&this.focus, cx);
                }))
                // Only the selected endpoint's rows arm the hover menu:
                // another endpoint's menu would have to select it first, and
                // resting the pointer must not switch which daemon is shown.
                // The feature is opt-in, so rows stay unarmed without it.
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
                .when(shift != px(0.), |row| row.top(shift));
                spaces = if carried {
                    // Painted last so it floats over the rows it passes, while
                    // its layout slot keeps the others' positions stable.
                    spaces.child(deferred(element.cursor_grabbing()).with_priority(1))
                } else {
                    spaces.child(element)
                };
            }
            if !self.config.show_agents {
                continue;
            }
            for agent in sorted_agents(&snapshot.agents, self.agent_sort) {
                if selected && agent.focused {
                    highlighted[1] = Some(agent_count);
                }
                agent_count += 1;
                let id = agent.pane_id.clone();
                let navigate_endpoint = endpoint_id.clone();
                let host = (multi && endpoint_id != crate::endpoint::LOCAL)
                    .then_some(endpoint.label.as_str());
                let (name, detail) = agent_labels(agent, snapshot, host);
                agents = agents.child(
                    row(
                        &format!("agent-{id}"),
                        &name,
                        detail,
                        RowKind::Agent(crate::icons::AgentIcon::from_identity(
                            agent.agent.as_deref(),
                        )),
                        agent.agent_status,
                        false,
                        selected && agent.focused,
                        RowLift::Resting,
                        RowTree::None,
                        false,
                        width,
                        RowIcon::None,
                        None,
                        None,
                        look,
                        (font, theme),
                    )
                    .id(SharedString::from(format!("agent-{endpoint_id}-{id}")))
                    .when(multi, |row| {
                        row.debug_selector(|| format!("agent-{endpoint_id}-{id}"))
                    })
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.navigate_endpoint(&navigate_endpoint, NavigationTarget::Pane(&id), cx);
                        window.focus(&this.focus, cx);
                    })),
                );
            }
        }
        if sliding {
            window.request_animation_frame();
        }
        self.reveal_highlighted(highlighted);
        if agent_count == 0 {
            agents = agents.child(
                div()
                    .px(px(content_x))
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
            .text_font(font)
            .text_size(px(font.size))
            .line_height(px(line_height(font)))
            .text_color(rgb(theme.foreground))
            .bg(rgb(theme.surface))
            .border_r_1()
            .border_color(rgb(theme.active))
            // Zero flex bases keep long workspace lists from displacing agents.
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
                    .child(header("spaces", font, theme, look))
                    .child(spaces)
                    .child(
                        div()
                            .flex_none()
                            .h(px(line_height(font) + 2. * layout.footer_padding()))
                            .px(px(content_x))
                            .flex()
                            .items_center()
                            // Menu hugs the sidebar's edge, as in the terminal client.
                            .justify_between()
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
            .when(self.config.show_agents, |sidebar| {
                sidebar
                    .child(
                        div()
                            .id("sidebar-split-resize")
                            .debug_selector(|| "sidebar-split-resize".into())
                            .h(px(6.))
                            .flex_none()
                            .cursor(CursorStyle::ResizeUpDown)
                            .border_t_1()
                            .border_color(rgb(theme.active))
                            .hover(|s| s.bg(rgba(0x78a9ff44)))
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|this, event: &MouseDownEvent, _, cx| {
                                    cx.stop_propagation();
                                    this.sidebar_split_modified = true;
                                    if event.click_count == 2 {
                                        this.sidebar_drag = None;
                                        this.sidebar_split = None;
                                        this.save_chrome();
                                    } else {
                                        this.sidebar_drag = Some(SidebarDrag::Split);
                                    }
                                    cx.notify();
                                }),
                            ),
                    )
                    .child(
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
                            .child(
                                header("agents", font, theme, look)
                                    .justify_between()
                                    .child(agents_sort(self, cx)),
                            )
                            .child(agents),
                    )
            })
            .child(self.render_device_footer(cx))
            .child(self.resize_handle(width, cx))
            .child(drag_capture(
                view,
                Drops {
                    rows: drop_rows,
                    requests: drop_requests,
                    dragged: drop_dragged,
                },
            ))
    }

    /// The handle on the sidebar's right edge. Dragging it resizes the
    /// sidebar, and past `COLLAPSE_BELOW` collapses or expands it; a double
    /// click restores the default width, or expands a collapsed sidebar.
    pub(super) fn resize_handle(&self, width: f32, cx: &mut Context<Self>) -> Stateful<Div> {
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
                    if event.click_count == 2 {
                        this.sidebar_drag = None;
                        if this.sidebar_mode == SidebarMode::Collapsed {
                            this.sidebar_mode = SidebarMode::Expanded;
                            this.sidebar_mode_modified = true;
                            this.save_chrome();
                        } else {
                            this.sidebar_width = None;
                            this.save_sidebar_width();
                        }
                    } else {
                        this.sidebar_drag = Some(SidebarDrag::Width {
                            start: f32::from(event.position.x),
                            width,
                            preferred: this.sidebar_width,
                        });
                    }
                    cx.notify();
                }),
            )
    }

    /// Applies a width drag. The sidebar collapses while the pointer would
    /// leave it narrower than `COLLAPSE_BELOW`, keeping the width it had so
    /// expanding by command restores it, and expands again past that line.
    fn drag_width(&mut self, drag: (f32, f32, Option<f32>), x: f32, window_width: f32) {
        let (start, width, preferred) = drag;
        let dragged = width + x - start;
        if dragged < COLLAPSE_BELOW {
            self.sidebar_mode = SidebarMode::Collapsed;
            self.sidebar_width = preferred;
        } else {
            self.sidebar_mode = SidebarMode::Expanded;
            self.sidebar_width = Some(sidebar_width(Some(dragged), window_width));
        }
        self.sidebar_modified = true;
        self.sidebar_mode_modified = true;
    }

    /// Scroll each list to its highlighted row once a frame has measured the
    /// viewport: the handle resolves the request against the previous frame's
    /// bounds, so an unmeasured list would scroll to a meaningless offset.
    /// Recording what was revealed keeps later frames from undoing the user's
    /// own scrolling.
    pub(super) fn reveal_highlighted(&self, highlighted: [Option<usize>; 2]) {
        for (list, row) in highlighted.iter().enumerate() {
            let Some(row) = *row else { continue };
            if self.sidebar_revealed[list].get() != Some(row)
                && self.sidebar_scroll[list].bounds().size.height > px(0.)
            {
                let scroll = &self.sidebar_scroll[list];
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
}

/// Where a lifted workspace row may drop: each drop unit's rows in the spaces
/// list, and the move every gap makes. The collapsed sidebar lifts no row, so
/// it passes none.
#[derive(Default)]
pub(super) struct Drops {
    rows: Vec<(usize, usize, Pixels)>,
    requests: Vec<Option<reorder::MoveBlock>>,
    dragged: usize,
}

/// Captures the pointer for the sidebar's drags: a lifted workspace row, the
/// width handle, and the split between the lists. Capturing globally keeps a
/// drag going outside the narrow handle, and keeps its release from reaching
/// the terminal.
pub(super) fn drag_capture(view: WeakEntity<HerdrWindow>, drops: Drops) -> Canvas<()> {
    canvas(
        |_, _, _| (),
        move |bounds, _, window, _| {
            let moving = view.clone();
            window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, cx| {
                if phase == DispatchPhase::Capture {
                    let _ = moving.update(cx, |this, cx| {
                        let scroll = this.sidebar_scroll[0].clone();
                        if this.move_workspace_drag(
                            event.position,
                            event.pressed_button == Some(MouseButton::Left),
                            |lift| {
                                reorder::resolve(
                                    &drops.rows,
                                    &drops.requests,
                                    drops.dragged,
                                    &scroll,
                                    lift,
                                )
                            },
                            cx,
                        ) {
                            cx.stop_propagation();
                        } else if let Some(drag) = this.sidebar_drag {
                            match drag {
                                SidebarDrag::Width {
                                    start,
                                    width,
                                    preferred,
                                } => this.drag_width(
                                    (start, width, preferred),
                                    f32::from(event.position.x),
                                    f32::from(window.viewport_size().width),
                                ),
                                SidebarDrag::Split => {
                                    let height =
                                        (f32::from(bounds.size.height) - 6. - DEVICE_FOOTER_HEIGHT)
                                            .max(1.);
                                    this.sidebar_split = Some(
                                        ((f32::from(event.position.y - bounds.origin.y) - 3.)
                                            / height)
                                            .clamp(0.1, 0.9),
                                    );
                                }
                            }
                            cx.stop_propagation();
                            cx.notify();
                        }
                    });
                }
            });
            let released = view.clone();
            window.on_mouse_event(move |event: &MouseUpEvent, phase, _, cx| {
                if phase == DispatchPhase::Capture && event.button == MouseButton::Left {
                    let _ = released.update(cx, |this, cx| {
                        // A lifted row's release is its drop, not a click.
                        if this.release_workspace_drag(cx) {
                            cx.stop_propagation();
                        } else if this.sidebar_drag.take().is_some() {
                            this.save_chrome();
                            cx.stop_propagation();
                            cx.notify();
                        }
                    });
                }
            });
        },
    )
    .absolute()
    .size_full()
}

pub(super) fn header(
    label: &'static str,
    font: &FontConfig,
    theme: &Theme,
    look: SidebarLook,
) -> Div {
    div()
        .debug_selector(|| format!("header-{label}"))
        .flex_none()
        .h(px(line_height(font) + 2. * look.density.header_padding()))
        .px(px(look.content_x()))
        .flex()
        .items_center()
        .text_size(px(font.size))
        .text_color(rgb(theme.muted))
        .child(
            div()
                .debug_selector(|| format!("header-label-{label}"))
                .child(label_text(&look.header_label(label))),
        )
}
