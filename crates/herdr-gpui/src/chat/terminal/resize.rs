//! Resize only the rich overlay, keeping the CLI's original cell coordinates.

use super::{ChatPane, InputSize, InputState, Layout};
use crate::{HerdrWindow, chat::Target};
use gpui::{prelude::*, *};

const HANDLE: f32 = 6.;

#[derive(Clone)]
struct InputDrag {
    generation: u64,
    target: Target,
}

impl InputDrag {
    fn matches(&self, pane: &ChatPane) -> bool {
        pane.target == self.target && pane.generation() == self.generation
    }
}

fn requested_rows(rows: u16, delta: Pixels, line_height: f32, height: u16) -> u16 {
    InputSize::Rows((f32::from(rows) - f32::from(delta) / line_height).round() as u16)
        .rows(height, false)
}

impl HerdrWindow {
    pub(super) fn resizable_chat_input(
        &self,
        bar: Stateful<Div>,
        chat: &ChatPane,
        layout: Layout,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        if layout.state == InputState::AwaitingFit {
            return bar;
        }
        let drag = InputDrag {
            generation: chat.generation(),
            target: chat.target.clone(),
        };
        let reset = drag.clone();
        let pane = chat.target.pane_id.clone();
        let height = layout.conversation.height + layout.divider.height + layout.input.height;
        let line_height = self.config.terminal.line_height();
        let entity = cx.entity();
        bar.on_drag_move(
            cx.listener(move |this, event: &DragMoveEvent<InputDrag>, _, cx| {
                let Some(drag) = event.dragged_item().downcast_ref::<InputDrag>() else {
                    return;
                };
                if drag.target.pane_id != pane {
                    return;
                }
                // Captured drag events also arrive outside this bar and pane.
                // Recompute against the currently painted divider after each
                // layout; neither window origins nor terminal cells are moved.
                cx.stop_propagation();
                let rows = requested_rows(
                    layout.input.height,
                    event.event.position.y - event.bounds.top(),
                    line_height,
                    height,
                );
                this.resize_chat_input(drag, InputSize::Rows(rows), cx);
            }),
        )
        .child(
            div()
                .id("chat-input-resize")
                .debug_selector(|| "chat-input-resize".into())
                .absolute()
                .top_0()
                .left_0()
                // Leave the capped 12px action label and its padding clickable,
                // including when a small terminal font makes this row shorter
                // than the resize handle.
                .right(px(120.))
                .h(px(HANDLE))
                .occlude()
                .cursor(CursorStyle::ResizeUpDown)
                .hover(|style| style.bg(rgba(crate::panel_resize::RESIZE_HOVER)))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, event: &MouseDownEvent, _, cx| {
                        cx.stop_propagation();
                        if event.click_count == 2 {
                            this.resize_chat_input(&reset, InputSize::Auto, cx);
                        }
                    }),
                )
                .on_drag(drag, move |drag, offset, _, cx| {
                    // Apply the first movement too: GPUI creates the typed
                    // drag after this event's capture phase has already run.
                    entity.update(cx, |this, cx| {
                        let rows =
                            requested_rows(layout.input.height, offset.y, line_height, height);
                        this.resize_chat_input(drag, InputSize::Rows(rows), cx);
                    });
                    cx.new(|_| EmptyView)
                }),
        )
    }

    fn resize_chat_input(&mut self, drag: &InputDrag, size: InputSize, cx: &mut Context<Self>) {
        if !self.chats.shown(&drag.target.pane_id) {
            return;
        }
        let Some(pane) = self.chats.pane_mut(&drag.target.pane_id) else {
            return;
        };
        if !drag.matches(pane) || pane.terminal_size == size {
            return;
        }
        pane.terminal_size = size;
        self.redraw_terminal(cx);
        cx.notify();
    }
}
