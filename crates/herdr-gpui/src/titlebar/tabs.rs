//! The title bar folded into the tab row, as Chrome and Conductor draw it.
//! With Herdr's tab bar at the top the window keeps no header of its own: the
//! sidebar's column starts with the traffic-light clearance, the sidebar
//! toggle, and Back and Forward, the leftmost group's strip takes them when
//! the sidebar is not expanded or too narrow for them, and the rightmost one ends with the bar's git button, account,
//! and window controls, with the header's usage text before them. Every
//! strip keeps empty room that moves the window, however many tabs it holds;
//! more tabs than fit still scroll.

use super::{HEIGHT, LEADING, movable, navigation::Style};
use crate::{HerdrWindow, browser::GroupId, herdr_settings::TabBarPosition, sidebar::SidebarMode};
use gpui::{prelude::*, *};

/// Empty room a strip keeps after its tabs so the window can always be
/// moved from it, even when its tabs overflow.
pub(crate) const DRAG_ROOM: f32 = 40.;

/// How readily the strip's leading controls give up width: far less than the
/// tabs, which shrink first, so they only narrow once the tabs have none.
/// Flex shrinking weighs this by the base width, so it stays negligible next
/// to any tab row.
const NAVIGATION_SHRINK: f32 = 0.001;

/// The sidebar toggle's 28px button and the 4px after it.
const SIDEBAR_TOGGLE: f32 = 32.;
/// Room kept after Back and Forward at the sidebar header's end.
const SIDEBAR_END: f32 = 8.;

/// Which window corners a group's strip reaches, so the bar's leading and
/// trailing parts land in the strips at the window's edges.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Ends {
    pub leading: bool,
    pub trailing: bool,
}

impl Ends {
    pub(crate) fn of(index: usize, count: usize) -> Self {
        Self {
            leading: index == 0,
            trailing: index + 1 == count,
        }
    }
}

impl HerdrWindow {
    /// Whether the tab row stands in for the header: Herdr puts the tab bar
    /// at the top and the leftmost group shows its strip. Otherwise the
    /// window keeps the full-width header.
    pub(crate) fn tabs_in_titlebar(&self, first: GroupId, cx: &App) -> bool {
        self.tab_bar_position() == TabBarPosition::Top && !self.strip_hidden(first, cx)
    }

    /// Whether Back and Forward fit in the expanded sidebar's header after
    /// the traffic lights and the toggle. A sidebar dragged narrower than
    /// that hands them to the leftmost strip instead.
    fn navigation_in_sidebar(&self, window: &Window) -> bool {
        let mode = self.sidebar_mode();
        let column = mode
            .width(self.sidebar_width, f32::from(window.viewport_size().width))
            .unwrap_or(0.);
        mode == SidebarMode::Expanded
            && column >= LEADING + SIDEBAR_TOGGLE + Style::NATIVE.width() + SIDEBAR_END
    }

    /// The sidebar column's first row, level with the strips beside it: the
    /// traffic lights' clearance, then the toggle and Back and Forward while
    /// the sidebar is expanded. A collapsed rail is too narrow for them, so
    /// the leftmost strip takes them and what remains of the clearance.
    pub(crate) fn sidebar_header(&self, window: &Window, cx: &mut Context<Self>) -> Div {
        let header = div()
            .debug_selector(|| "sidebar-titlebar".into())
            .flex()
            .flex_none()
            .items_center()
            .w_full()
            .h(px(self.tab_strip_height()))
            .overflow_hidden()
            .bg(rgb(self.theme.sidebar_background()))
            .child(div().flex_none().w(px(LEADING)).h_full())
            .when(self.sidebar_mode() == SidebarMode::Expanded, |header| {
                header.child(self.sidebar_toggle(cx))
            })
            .when(self.navigation_in_sidebar(window), |header| {
                header.child(self.navigation(cx))
            });
        movable(header, window)
    }

    /// The sidebar under its header row, at the width the sidebar takes.
    pub(crate) fn sidebar_column(
        &self,
        sidebar: impl IntoElement,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Div {
        let width = self
            .sidebar_mode()
            .width(self.sidebar_width, f32::from(window.viewport_size().width))
            .unwrap_or(0.);
        div()
            .flex()
            .flex_col()
            .flex_none()
            .w(px(width))
            .min_h_0()
            .child(self.sidebar_header(window, cx))
            .child(div().flex().flex_1().min_h_0().child(sidebar))
    }

    /// The development build's worktree banner.
    pub(crate) fn render_worktree_banner(&self) -> Option<Div> {
        crate::worktree_banner::render(
            env!("HERDR_BUILD_WORKTREE") == "1",
            env!("HERDR_BUILD_BRANCH"),
            env!("HERDR_BUILD_PR"),
        )
    }

    /// What leads the leftmost strip: whatever clearance the sidebar column
    /// leaves the traffic lights, then the toggle and Back and Forward when
    /// the sidebar header does not show them. Nothing when it shows both.
    pub(crate) fn strip_leading(&self, window: &Window, cx: &mut Context<Self>) -> Option<Div> {
        if self.navigation_in_sidebar(window) {
            return None;
        }
        // Unlike the header, the strip shares its row with tabs and the
        // trailing controls, at a width set by the sidebar, panels and group
        // shares. The leading part may shrink, after the tabs and before
        // anything else, so the window controls are never pushed out.
        let leading = div()
            .debug_selector(|| "strip-titlebar-leading".into())
            // These controls belong to the window. Exclude the group's
            // capture handler before it can switch connections and history;
            // stopping propagation in a button's bubble handler is too late.
            .block_mouse_except_scroll()
            .flex()
            .flex_shrink(NAVIGATION_SHRINK)
            .min_w_0()
            .overflow_hidden()
            .items_center();
        let mode = self.sidebar_mode();
        // An expanded sidebar too narrow for Back and Forward keeps the
        // toggle, and they open the content beside it.
        if mode == SidebarMode::Expanded {
            return Some(leading.pl(px(6.)).child(self.strip_navigation(cx)));
        }
        let column = mode
            .width(self.sidebar_width, f32::from(window.viewport_size().width))
            .unwrap_or(0.);
        Some(
            leading
                .child(movable(
                    div()
                        .flex_none()
                        .self_stretch()
                        .w(px((LEADING - column).max(0.))),
                    window,
                ))
                .child(self.sidebar_toggle(cx))
                .child(self.strip_navigation(cx)),
        )
    }

    /// Back and Forward in a slot that keeps their width but is the one part
    /// of the leading controls that can shrink. They draw only when the slot
    /// last got its full width, so a squeezed strip drops them whole rather
    /// than clipping a button. The slot's size never depends on whether they
    /// draw, so the choice cannot flip from one frame to the next.
    fn strip_navigation(&self, cx: &mut Context<Self>) -> Div {
        let width = Style::NATIVE.width();
        let view = cx.entity().downgrade();
        div()
            .relative()
            .flex()
            .items_center()
            .self_stretch()
            .w(px(width))
            .min_w_0()
            // The only part of the leading controls that shrinks, so all of
            // their lost width comes out of it.
            .flex_shrink(1.)
            .overflow_hidden()
            .child(
                canvas(
                    move |bounds, _, cx| {
                        let fits = bounds.size.width >= px(width - 0.5);
                        let _ = view.update(cx, |this, cx| {
                            if this.strip_navigation_fits != fits {
                                this.strip_navigation_fits = fits;
                                cx.notify();
                            }
                        });
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .size_full(),
            )
            .when(self.strip_navigation_fits, |slot| {
                slot.child(self.navigation(cx))
            })
    }

    /// What ends the rightmost strip: the same controls the header ends with.
    pub(crate) fn strip_trailing(&self, window: &Window, cx: &mut Context<Self>) -> Div {
        self.titlebar_end(window, cx)
            .debug_selector(|| "strip-titlebar-trailing".into())
            .pl(px(4.))
    }

    /// The room after a strip's tabs: it moves the window when the strip
    /// stands in for the header, and simply fills the row otherwise. The
    /// rightmost one also shows the usage the header's center would, which
    /// truncates before the room gets too small to grab.
    pub(crate) fn strip_room(&self, ends: Option<Ends>, window: &Window) -> Div {
        let room = div().flex_1().min_w_0();
        let Some(ends) = ends else {
            return room;
        };
        let room = room
            .debug_selector(|| "strip-titlebar-room".into())
            .flex()
            .pl(px(DRAG_ROOM))
            .when(ends.trailing && self.config.usage.topbar, |room| {
                room.child(super::status::render(
                    &self.live,
                    &self.config.ui,
                    &self.theme,
                ))
            });
        movable(room, window)
    }
}

/// The strip height that lines up with the traffic lights when the strip
/// stands in for the header.
pub(crate) fn strip_height(base: f32, merged: bool) -> f32 {
    if merged { base.max(HEIGHT) } else { base }
}

#[cfg(test)]
mod tests;
