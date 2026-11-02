//! The workspace popover: a header naming the workspace, with the toggle
//! that mutes its notifications, a fixed grid of large action tiles, plain
//! rows for the rarer actions, the destructive action set apart, and the pull
//! request section below.

// i18n: migrated
use crate::i18n::fl;
use crate::{
    HerdrWindow, i18n,
    menu::{WorkspaceAction, WorkspaceMenuAction, accent, danger},
};
use gpui::{prelude::*, *};

/// An action's menu label: a message ID, shown in the current language. Its
/// English names the action's elements, so tests find them in any language.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Label(&'static str);

impl Label {
    // keep-sorted start
    pub(crate) const CHECKPOINTS: Self = Self::new("worktree-popover-checkpoints");
    pub(crate) const CLEAR_TELEPORTED: Self = Self::new("worktree-popover-clear-teleported");
    pub(crate) const CLOSE: Self = Self::new("worktree-popover-close");
    pub(crate) const CLOSE_GROUP: Self = Self::new("worktree-popover-close-group");
    pub(crate) const COLLAPSE_GROUP: Self = Self::new("worktree-popover-collapse-group");
    pub(crate) const DELETE_WORKTREE: Self = Self::new("worktree-popover-delete-worktree");
    pub(crate) const EXPAND_GROUP: Self = Self::new("worktree-popover-expand-group");
    pub(crate) const FAN_OUT: Self = Self::new("worktree-popover-fan-out");
    pub(crate) const FAN_OUT_COMPARISON: Self = Self::new("worktree-popover-fan-out-comparison");
    pub(crate) const GO_TO_TELEPORTED: Self = Self::new("worktree-popover-go-to-teleported");
    pub(crate) const MUTE: Self = Self::new("worktree-popover-mute");
    pub(crate) const NEW_WORKTREE: Self = Self::new("worktree-popover-new-worktree");
    pub(crate) const NOTE: Self = Self::new("worktree-popover-note");
    pub(crate) const OPEN_WORKTREE: Self = Self::new("worktree-popover-open-worktree");
    pub(crate) const ORCHESTRATOR: Self = Self::new("worktree-popover-orchestrator");
    pub(crate) const RENAME: Self = Self::new("worktree-popover-rename");
    pub(crate) const RUN_SCRIPT: Self = Self::new("worktree-popover-run-script");
    pub(crate) const RUN_SETUP_SCRIPT: Self = Self::new("worktree-popover-run-setup-script");
    pub(crate) const TELEPORT: Self = Self::new("worktree-popover-teleport");
    pub(crate) const TELEPORT_BACK: Self = Self::new("worktree-popover-teleport-back");
    pub(crate) const UNMUTE: Self = Self::new("worktree-popover-unmute");
    // keep-sorted end
    #[cfg(feature = "cloud")]
    pub(crate) const SANDBOX_IMAGES: Self = Self::new("worktree-popover-sandbox-images");

    /// Checked against the English files as it compiles, as `fl!` is.
    const fn new(id: &'static str) -> Self {
        i18n::check(id, &[]);
        Self(id)
    }

    /// The label in the current language.
    pub(crate) fn text(self) -> String {
        i18n::message(self.0, &[])
    }

    /// What a screen reader says: the label without the "..." that marks a
    /// row opening a dialog.
    pub(crate) fn spoken(self) -> String {
        let text = self.text();
        text.trim_end_matches("...")
            .trim_end_matches('\u{2026}')
            .to_owned()
    }

    /// The English label, which names the action's elements in tests.
    pub(crate) fn english(self) -> String {
        i18n::english(self.0, &[])
    }

    /// The element ID: the message ID, the same in every language.
    fn id(self) -> &'static str {
        self.0
    }
}

/// The grid's fixed slots, row by row: working on the checkout, then moving
/// or leaving it. A slot lists the actions it can show, first available
/// first, and the action it shows grayed when none is.
const SLOTS: [(&[WorkspaceMenuAction], WorkspaceMenuAction, Label); 6] = {
    use WorkspaceAction::*;
    use WorkspaceMenuAction::*;
    [
        (
            &[Dialog(NewWorktree)],
            Dialog(NewWorktree),
            Label::NEW_WORKTREE,
        ),
        (&[FanOut], FanOut, Label::FAN_OUT),
        (&[Dialog(Rename)], Dialog(Rename), Label::RENAME),
        (&[GoToTeleported, Teleport], Teleport, Label::TELEPORT),
        (
            &[ClearTeleported, TeleportBack],
            TeleportBack,
            Label::TELEPORT_BACK,
        ),
        (&[Dialog(Close)], Dialog(Close), Label::CLOSE),
    ]
};

const COLUMNS: usize = 3;

/// The rows under the grid share one icon column and one label column: the
/// same inset, a fixed icon slot each icon centres in whatever its size, and
/// the same gap before the label.
const ROW_INSET: f32 = 8.;
const ICON_SLOT: f32 = 20.;
const ICON_SIZE: f32 = 16.;
const ROW_GAP: f32 = 8.;

/// A row's leading icon, centred in the shared slot. An action without an
/// icon keeps the empty slot so its label still lines up.
fn leading_icon(action: WorkspaceMenuAction, label: Label, color: Rgba) -> Div {
    div()
        .size(px(ICON_SLOT))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .when_some(action.icon(), |slot, icon| {
            slot.child(
                svg()
                    .path(icon)
                    .size(px(ICON_SIZE))
                    .flex_none()
                    .text_color(color)
                    .debug_selector(move || format!("workspace-menu-icon-{}", label.english())),
            )
        })
}

/// One grid cell: the action it runs, its menu label, and whether this
/// workspace can take it now. A grayed tile keeps the grid's shape stable.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::menu) struct Tile {
    pub(in crate::menu) action: WorkspaceMenuAction,
    pub(in crate::menu) label: Label,
    pub(in crate::menu) enabled: bool,
}

impl Tile {
    /// The tile's caption: the menu label, shortened where the tile's icon
    /// and position already say the rest.
    pub(in crate::menu) fn caption(self) -> String {
        match self.label {
            Label::NEW_WORKTREE => fl!("worktree-popover-tile-new-worktree"),
            Label::FAN_OUT => fl!("worktree-popover-tile-fan-out"),
            Label::FAN_OUT_COMPARISON => fl!("worktree-popover-tile-comparison"),
            Label::RENAME => fl!("worktree-popover-tile-rename"),
            Label::GO_TO_TELEPORTED => fl!("worktree-popover-tile-go-to-teleported"),
            Label::CLEAR_TELEPORTED => fl!("worktree-popover-tile-clear-teleported"),
            Label::TELEPORT => fl!("worktree-popover-tile-teleport"),
            Label::TELEPORT_BACK => fl!("worktree-popover-tile-teleport-back"),
            Label::CLOSE => fl!("worktree-popover-tile-close"),
            Label::CLOSE_GROUP => fl!("worktree-popover-tile-close-group"),
            label => label.spoken(),
        }
    }
}

/// Lays `items` into the grid. Items no slot takes are returned in order,
/// for the rows beneath it, except what the header draws.
pub(in crate::menu) fn layout(
    items: &[(WorkspaceMenuAction, Label)],
) -> ([Tile; 6], Vec<(WorkspaceMenuAction, Label)>) {
    let tiles = SLOTS.map(|(choices, fallback, label)| {
        choices
            .iter()
            .find_map(|choice| items.iter().find(|(action, _)| action == choice))
            .map_or(
                Tile {
                    action: fallback,
                    label,
                    enabled: false,
                },
                |&(action, label)| Tile {
                    action,
                    label,
                    enabled: true,
                },
            )
    });
    let rest = items
        .iter()
        .filter(|(action, _)| {
            !action.in_header()
                && !tiles
                    .iter()
                    .any(|tile| tile.enabled && tile.action == *action)
        })
        .copied()
        .collect();
    (tiles, rest)
}

/// The popover's width in English, where every caption fits its tile.
const WIDTH: f32 = 340.;

/// What the popover adds around its grid (inset, border, and the grid's own
/// padding), and around each caption (the tile's padding).
const PANEL_INSET: f32 = 28.;
const TILE_INSET: f32 = 8.;
const TILE_GAP: f32 = 6.;

impl HerdrWindow {
    /// The popover's width: 340, or as wide as three of the longest caption
    /// need in a longer language, and never wider than `room`. Captions are
    /// short and few, and GPUI caches each shaped line, so measuring them as
    /// the popover draws costs no more than drawing them.
    pub(in crate::menu) fn workspace_popover_width(&self, window: &Window, room: Pixels) -> Pixels {
        let size = px((self.config.ui.size - 1.).max(8.));
        let font = self.config.ui.font();
        let (tiles, _) = layout(&self.workspace_items());
        let widest = tiles
            .iter()
            .map(|tile| {
                let caption = tile.caption();
                let mut run = window.text_style().to_run(caption.len());
                run.font = font.clone();
                window
                    .text_system()
                    .shape_line(caption.into(), size, &[run], None)
                    .width
            })
            .max()
            .unwrap_or_default();
        let columns = COLUMNS as f32;
        let needed =
            (widest + px(TILE_INSET)) * columns + px(TILE_GAP * (columns - 1.) + PANEL_INSET);
        needed.max(px(WIDTH)).min(room)
    }

    pub(in crate::menu) fn render_workspace_popover(
        &self,
        pr_width: Pixels,
        cx: &mut Context<Self>,
    ) -> Div {
        let theme = &self.theme;
        let font = &self.config.ui;
        let items = self.workspace_items();
        let mut panel = div().flex().flex_col();
        if let Some(target) = &self.menu.target {
            let names = div()
                .flex_1()
                .min_w_0()
                .child(
                    div()
                        .debug_selector(|| "workspace-menu-name".into())
                        // Long names wrap: the header is the one place the
                        // full name shows when the sidebar row cuts it off.
                        .whitespace_normal()
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(crate::sidebar::label_text(&target.label)),
                )
                .when_some(
                    target
                        .branch
                        .as_deref()
                        .filter(|branch| !branch.trim().is_empty()),
                    |names, branch| {
                        names.child(
                            div()
                                .debug_selector(|| "workspace-menu-branch".into())
                                .truncate()
                                .text_color(rgb(theme.muted))
                                .text_size(px(font.size * 0.9))
                                .child(crate::sidebar::label_text(branch)),
                        )
                    },
                );
            panel = panel.child(
                div()
                    .debug_selector(|| "workspace-menu-header".into())
                    .px(px(8.))
                    .py(px(6.))
                    .mb(px(4.))
                    .border_b_1()
                    .border_color(rgb(theme.active))
                    .flex()
                    .items_start()
                    .gap(px(8.))
                    .child(names)
                    .children(
                        items
                            .iter()
                            .find(|(action, _)| action.in_header())
                            .map(|&(action, label)| self.render_workspace_mute(action, label, cx)),
                    ),
            );
        }
        let (tiles, rest) = layout(&items);
        let mut grid = div()
            .debug_selector(|| "workspace-menu-tiles".into())
            .flex()
            .flex_col()
            .gap(px(6.))
            .p(px(2.));
        for row in tiles.chunks(COLUMNS) {
            grid = grid.child(
                div()
                    .flex()
                    .gap(px(6.))
                    .children(row.iter().map(|tile| self.render_workspace_tile(*tile, cx))),
            );
        }
        panel = panel.child(grid);
        let delete = WorkspaceMenuAction::Dialog(WorkspaceAction::DeleteWorktree);
        let (rows, destructive): (Vec<_>, Vec<_>) =
            rest.into_iter().partition(|(action, _)| *action != delete);
        if !rows.is_empty() {
            panel = panel.child(div().h(px(6.)));
        }
        for (action, label) in rows {
            panel = panel.child(self.render_workspace_row(action, label, cx));
        }
        for (action, label) in destructive {
            panel = panel.child(self.render_workspace_danger(action, label, cx));
        }
        if self.pr_profile().is_some() {
            panel = panel.child(self.render_workspace_pr(pr_width, cx));
        }
        panel
    }

    /// The header's toggle for the workspace's notifications, `action` with
    /// its menu `label`: what pressing it does, with the bell it leaves. In
    /// the header, so muting adds no row that would push the pull request
    /// card out of a short window; still first in the arrow keys' walk.
    /// Muted, it keeps the accent, so the state reads at a glance.
    pub(in crate::menu) fn render_workspace_mute(
        &self,
        action: WorkspaceMenuAction,
        label: Label,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let theme = &self.theme;
        let muted = action == WorkspaceMenuAction::Unmute;
        let selected = Some(action) == self.menu.workspace_selected;
        let ink = if muted {
            accent(theme)
        } else {
            rgb(theme.muted)
        };
        // One line of the header tall, so it never makes the header taller.
        let line = self.config.ui.line_height();
        div()
            .id(label.id())
            .debug_selector(move || format!("workspace-menu-{}", label.english()))
            .role(Role::Button)
            .aria_label(label.text())
            .when(selected, |toggle| toggle.aria_active_descendant())
            .flex_none()
            .h(px(line))
            .flex()
            .items_center()
            .gap(px(4.))
            .px(px(6.))
            .rounded(px(crate::config::corners::CONTROL))
            .border_1()
            .border_color(Rgba { a: 0.5, ..ink })
            .map(|toggle| match (selected, muted) {
                (true, _) => toggle.bg(rgb(theme.active)),
                (false, true) => toggle.bg(Rgba { a: 0.14, ..ink }),
                (false, false) => toggle,
            })
            .text_color(ink)
            .text_size(px((self.config.ui.size - 2.).max(8.)))
            .cursor_pointer()
            .on_hover(Self::workspace_hover(action, cx))
            .when_some(action.icon(), |toggle, icon| {
                toggle.child(
                    svg()
                        .path(icon)
                        .size(px((line - 6.).clamp(8., ICON_SIZE - 4.)))
                        .flex_none()
                        .text_color(ink)
                        .debug_selector(|| "workspace-menu-mute-icon".into()),
                )
            })
            .child(if muted {
                fl!("worktree-popover-unmute-button")
            } else {
                fl!("worktree-popover-mute-button")
            })
            .on_click(cx.listener(move |this, _, window, cx| {
                cx.stop_propagation();
                this.activate_workspace_menu(action, window, cx);
            }))
    }

    /// Hover selects, as the arrow keys do, so both highlight the same item.
    fn workspace_hover(
        action: WorkspaceMenuAction,
        cx: &mut Context<Self>,
    ) -> impl Fn(&bool, &mut Window, &mut App) + 'static {
        cx.listener(move |this, hovered: &bool, _, cx| {
            if *hovered {
                this.menu.workspace_selected = Some(action);
            } else if this.menu.workspace_selected == Some(action) {
                this.menu.workspace_selected = None;
            }
            cx.notify();
        })
    }

    fn render_workspace_tile(&self, tile: Tile, cx: &mut Context<Self>) -> Stateful<Div> {
        let theme = &self.theme;
        let label = tile.label;
        let selected = tile.enabled && self.menu.workspace_selected == Some(tile.action);
        let primary = tile.action == WorkspaceMenuAction::Dialog(WorkspaceAction::NewWorktree);
        let ink = if primary && tile.enabled {
            accent(theme)
        } else {
            rgb(theme.foreground)
        };
        let resting = if primary && tile.enabled {
            Rgba { a: 0.16, ..ink }
        } else {
            rgb(theme.surface).blend(rgba((theme.foreground << 8) | 0x0f))
        };
        crate::accessibility::disabled(
            div()
                .id(label.id())
                .role(Role::MenuItem)
                .aria_label(label.spoken())
                .when(selected, |tile| tile.aria_active_descendant()),
            !tile.enabled,
        )
        .debug_selector(move || format!("workspace-menu-{}", label.english()))
        .flex_1()
        .min_w_0()
        .min_h(px(64.))
        .px(px(4.))
        .py(px(10.))
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap(px(6.))
        .rounded(px(crate::config::corners::CONTROL))
        .bg(if selected { rgb(theme.active) } else { resting })
        .text_color(ink)
        .text_size(px((self.config.ui.size - 1.).max(8.)))
        .when_some(tile.action.icon(), |tile_div, icon| {
            tile_div.child(
                svg()
                    .path(icon)
                    .size(px(20.))
                    .flex_none()
                    .text_color(ink)
                    .debug_selector(move || format!("workspace-menu-icon-{}", label.english())),
            )
        })
        .child(
            div()
                .debug_selector(move || format!("workspace-menu-label-{}", label.english()))
                .max_w_full()
                .truncate()
                .child(tile.caption()),
        )
        .map(|tile_div| {
            if !tile.enabled {
                return tile_div.opacity(0.4);
            }
            let action = tile.action;
            tile_div
                .cursor_pointer()
                .on_hover(Self::workspace_hover(action, cx))
                .on_click(cx.listener(move |this, _, window, cx| {
                    cx.stop_propagation();
                    this.activate_workspace_menu(action, window, cx);
                }))
        })
    }

    /// A rarer action, as a plain row. Its icon leads, in the same column as
    /// the destructive strip's, so the rows read as one list under the tiles.
    fn render_workspace_row(
        &self,
        action: WorkspaceMenuAction,
        label: Label,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let theme = &self.theme;
        let selected = Some(action) == self.menu.workspace_selected;
        div()
            .id(label.id())
            .debug_selector(move || format!("workspace-menu-{}", label.english()))
            .role(Role::MenuItem)
            .aria_label(label.spoken())
            .when(selected, |row| row.aria_active_descendant())
            .mx(px(2.))
            .min_h(px(self.config.ui.line_height() + 12.))
            .px(px(ROW_INSET))
            .flex()
            .items_center()
            .gap(px(ROW_GAP))
            .cursor_pointer()
            .rounded(px(crate::config::corners::CONTROL))
            .when(selected, |row| row.bg(rgb(theme.active)))
            .on_hover(Self::workspace_hover(action, cx))
            .child(leading_icon(
                action,
                label,
                rgb(if selected {
                    theme.foreground
                } else {
                    theme.muted
                }),
            ))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .debug_selector(move || format!("workspace-menu-label-{}", label.english()))
                    .child(label.text()),
            )
            .on_click(cx.listener(move |this, _, window, cx| {
                cx.stop_propagation();
                this.activate_workspace_menu(action, window, cx);
            }))
    }

    /// The destructive action, full width and red, apart from the tiles so it
    /// never reads as one more of them.
    fn render_workspace_danger(
        &self,
        action: WorkspaceMenuAction,
        label: Label,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let theme = &self.theme;
        let red = danger(theme);
        let selected = Some(action) == self.menu.workspace_selected;
        div()
            .id(label.id())
            .debug_selector(move || format!("workspace-menu-{}", label.english()))
            .role(Role::MenuItem)
            .aria_label(label.spoken())
            .when(selected, |row| row.aria_active_descendant())
            .mt(px(6.))
            .mx(px(2.))
            .min_h(px(self.config.ui.line_height() + 12.))
            .px(px(ROW_INSET))
            .flex()
            .items_center()
            .gap(px(ROW_GAP))
            .cursor_pointer()
            .rounded(px(crate::config::corners::CONTROL))
            .bg(Rgba {
                a: if selected { 0.26 } else { 0.12 },
                ..red
            })
            .text_color(red)
            .on_hover(Self::workspace_hover(action, cx))
            .child(leading_icon(action, label, red))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .debug_selector(move || format!("workspace-menu-label-{}", label.english()))
                    .child(label.text()),
            )
            .on_click(cx.listener(move |this, _, window, cx| {
                cx.stop_propagation();
                this.activate_workspace_menu(action, window, cx);
            }))
    }
}
