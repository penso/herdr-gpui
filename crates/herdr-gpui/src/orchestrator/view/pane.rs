//! The details pane beside a list: its width bounds, the handle on its left
//! edge, and what dragging that handle asks for. The Pull requests, Issues,
//! and Security sections share it, so their panes open and resize alike.

use crate::config::Theme;
use gpui::{prelude::*, *};

/// The pane's width bounds, in pixels, and its largest share of the window.
pub(crate) const PANE_MIN: f32 = 360.;
pub(crate) const PANE_DEFAULT: f32 = 520.;
const PANE_MAX_SHARE: f32 = 0.7;

/// The pane's drag handle, as a drag payload.
#[derive(Clone, Copy)]
pub(crate) struct PaneDrag;

impl Render for PaneDrag {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        Empty
    }
}

/// What a drag of the handle asks for.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Resize {
    /// Dragged nearly shut: close the pane.
    Close,
    Width(f32),
}

impl Resize {
    /// The pane's new width for a drag within `event`'s container, whose
    /// right edge the pane keeps.
    pub(crate) fn of(event: &DragMoveEvent<PaneDrag>, window: &Window) -> Self {
        let width = f32::from(event.bounds.right() - event.event.position.x);
        let max = f32::from(window.viewport_size().width) * PANE_MAX_SHARE;
        if width < PANE_MIN / 2. {
            Self::Close
        } else {
            Self::Width(width.clamp(PANE_MIN, max.max(PANE_MIN)))
        }
    }
}

/// The handle on the pane's left edge.
pub(crate) fn handle(theme: &Theme, id: &'static str) -> Stateful<Div> {
    div()
        .id(id)
        .debug_selector(move || id.into())
        .flex_none()
        .w(px(6.))
        .h_full()
        .flex()
        .items_center()
        .justify_center()
        .border_l_1()
        .border_color(rgb(theme.active))
        .cursor(CursorStyle::ResizeLeftRight)
        .on_drag(PaneDrag, |drag, _, _, cx| cx.new(|_| *drag))
        .child(
            div()
                .w(px(2.))
                .h(px(28.))
                .rounded_full()
                .bg(rgb(theme.active)),
        )
}
