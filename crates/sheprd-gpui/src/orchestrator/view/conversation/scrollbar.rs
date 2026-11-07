//! The conversation's scrollbar: the thumb a terminal pane draws, along the
//! list's right edge, which shows where the view sits and drags it. Rows are
//! measured as they lay out, so its size settles as the conversation scrolls.
use super::OrchestratorView;
use crate::browser::Thumb;
use gpui::{prelude::*, *};

/// The painted thumb's width and margin, as a pane draws it.
const WIDTH: f32 = 4.;
const INSET: f32 = 1.;
/// How wide the thumb can be grabbed, wider than it draws.
const GRIP: f32 = 8.;
/// How opaque the thumb draws over the muted color, as a pane's does.
const ALPHA: u32 = 0xc0;

/// What the conversation's thumb drags.
#[derive(Clone, Copy, Debug)]
struct ConversationThumb;

/// The thumb for `list` as it last laid out; none while it all fits.
fn thumb(list: &ListState) -> Option<(Thumb, Bounds<Pixels>, f32)> {
    let bounds = list.viewport_bounds();
    let max = f32::from(list.max_offset_for_scrollbar().y);
    let thumb = Thumb::of(
        f32::from(bounds.size.height),
        max,
        -f32::from(list.scroll_px_offset_for_scrollbar().y),
    )?;
    Some((thumb, bounds, max))
}

/// Draws again when the list, laid out after the thumb was placed, moved it:
/// the first frame, a resize, or rows measured on the way.
fn keep_current(list: ListState, drawn: Option<Thumb>) -> impl IntoElement {
    canvas(
        move |_, window, _| {
            if thumb(&list).map(|(thumb, _, _)| thumb) != drawn {
                window.refresh();
            }
        },
        |_, _, _, _| {},
    )
    .absolute()
    .size_0()
}

impl OrchestratorView {
    fn grab_conversation_thumb(&mut self, y: Pixels) {
        if let Some((thumb, bounds, _)) = thumb(&self.conversation.list()) {
            self.conversation.grab = f32::from(y - bounds.top()) - thumb.left;
        }
    }

    /// Moves the thumb with the pointer at `y`; whether the list scrolled.
    fn drag_conversation_thumb(&self, y: Pixels) -> bool {
        let list = self.conversation.list();
        let Some((thumb, bounds, max)) = thumb(&list) else {
            return false;
        };
        let top = f32::from(y - bounds.top()) - self.conversation.grab;
        let scrolled = thumb.scrolled_at(f32::from(bounds.size.height), max, top);
        let offset = list.scroll_px_offset_for_scrollbar();
        if (f32::from(offset.y) + scrolled).abs() < 0.5 {
            return false;
        }
        list.set_offset_from_scrollbar(point(offset.x, px(-scrolled)));
        true
    }

    /// `list` with the scrollbar over its right edge.
    pub(super) fn conversation_scroll_area(
        &self,
        list: impl IntoElement,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let color = rgba((self.look.theme.muted << 8) | ALPHA);
        let state = self.conversation.list();
        let drawn = thumb(&state).map(|(thumb, _, _)| thumb);
        let grip = drawn.map(|thumb| {
            div()
                .id("orchestrator-conversation-thumb")
                .debug_selector(|| "orchestrator-conversation-thumb".into())
                .absolute()
                .right_0()
                .top(px(thumb.left))
                .w(px(GRIP))
                .h(px(thumb.width))
                // The wheel still scrolls the conversation over the thumb.
                .block_mouse_except_scroll()
                .child(
                    div()
                        .absolute()
                        .top_0()
                        .bottom_0()
                        .right(px(INSET))
                        .w(px(WIDTH))
                        .rounded_full()
                        .bg(color),
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, event: &MouseDownEvent, _, cx| {
                        cx.stop_propagation();
                        this.grab_conversation_thumb(event.position.y);
                    }),
                )
                .on_drag(ConversationThumb, |_, _, _, cx| cx.new(|_| EmptyView))
        });
        div()
            .id("orchestrator-conversation-scroll-area")
            .relative()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .flex()
            .flex_col()
            .on_drag_move(
                cx.listener(|this, event: &DragMoveEvent<ConversationThumb>, _, cx| {
                    if this.drag_conversation_thumb(event.event.position.y) {
                        cx.notify();
                    }
                }),
            )
            .child(list)
            .child(keep_current(state, drawn))
            .children(grip)
    }
}
