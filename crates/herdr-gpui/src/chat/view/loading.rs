//! A first attachment covers only old output, never the original editor.

use super::*;
use crate::chat::terminal::InputState;
use herdr_client::protocol::{AgentStatus, PaneSurfacePane};

impl HerdrWindow {
    pub(super) fn render_chat_loading(
        &self,
        pane: &PaneSurfacePane,
        surface: &PaneSurfaceFrame,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        // A clipped editor reports AwaitingFit instead of Blocked in the
        // geometry. Its live approval/question still must remain uncovered.
        if self.chats.raw(&pane.pane_id)
            || self.live.agent_status(&pane.pane_id) == Some(AgentStatus::Blocked)
        {
            return None;
        }
        let chat = self.chats.pane(&pane.pane_id);
        let discovering = chat.is_none() && self.chats.existing.loading(&pane.pane_id);
        if !discovering && !chat.is_some_and(ChatPane::loading) {
            return None;
        }
        let layout = self.existing_terminal_layout(
            pane,
            surface,
            chat.map(|chat| chat.terminal_size).unwrap_or_default(),
        );
        // Never cover a question/approval or an editor whose actual caret
        // already uses the conversation area. Input routing stays untouched.
        if layout.conversation.height == 0 {
            return None;
        }
        let bounds = pane_bounds(
            layout.conversation,
            self.cell_width,
            self.config.terminal.line_height(),
        );
        let pane_id = pane.pane_id.clone();
        let focus_pane = pane_id.clone();
        let theme = &self.theme;
        Some(
            div()
                .id(SharedString::from(format!("chat-loading-{}", pane.pane_id)))
                .debug_selector(|| "chat-loading".into())
                .absolute()
                .left(bounds.origin.x)
                .top(bounds.origin.y)
                .w(bounds.size.width)
                .h(bounds.size.height)
                .occlude()
                .overflow_hidden()
                .bg(rgb(theme.background))
                .text_font(&self.config.ui)
                .text_size(px(self.chat_text_size()))
                .text_color(rgb(theme.foreground))
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap(px(10.))
                .p(px(16.))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _, window, cx| {
                        cx.stop_propagation();
                        this.focus_existing_chat_input(&focus_pane, window, cx);
                    }),
                )
                .child(if discovering {
                    "Finding conversation…"
                } else {
                    "Loading conversation…"
                })
                .child(
                    div()
                        .text_color(rgb(theme.subtext()))
                        .text_size(px(self.chat_text_size().min(13.)))
                        .child(if layout.state == InputState::AwaitingFit {
                            "Your agent keeps running. Activate this pane to fit its input."
                        } else {
                            "Your agent and its input remain available below."
                        }),
                )
                .child(
                    div()
                        .id("chat-loading-terminal")
                        .debug_selector(|| "chat-loading-terminal".into())
                        .cursor_pointer()
                        .px(px(10.))
                        .py(px(4.))
                        .rounded(px(corners::CONTROL))
                        .bg(rgb(theme.surface))
                        .hover(|button| button.bg(rgb(theme.active)))
                        .child("Use terminal")
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                            cx.stop_propagation();
                            this.focus_existing_chat_input(&pane_id, window, cx);
                            if this.chats.pane(&pane_id).is_some() {
                                this.set_chat_raw(&pane_id, true, window, cx);
                            } else {
                                this.chats.existing.dismiss(&pane_id);
                                this.redraw_terminal(cx);
                            }
                        })),
                )
                .into_any_element(),
        )
    }
}
