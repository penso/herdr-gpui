//! An explicit way back to following after reading earlier transcript output.

use super::ChatPane;
use crate::HerdrWindow;
use gpui::{prelude::*, *};

impl HerdrWindow {
    pub(super) fn render_chat_jump(
        &self,
        chat: &ChatPane,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let paused = !chat.selection.is_empty() || chat.selection.is_selecting();
        if !chat.glide.needs_jump(&chat.list, paused) {
            return None;
        }
        let pane = chat.target.pane_id.clone();
        let theme = &self.theme;
        let ink = crate::contrast::ink_on_chrome(
            theme.palette[6],
            [theme.surface, theme.active, theme.surface],
            4.5,
        );
        Some(
            div()
                .absolute()
                .left(px(0.))
                .w_full()
                .bottom(px(12.))
                .flex()
                .justify_center()
                .child(
                    div()
                        .map(|button| {
                            #[cfg(feature = "integration-test")]
                            {
                                crate::chat::preview::jump::track_button(&pane, button)
                            }
                            #[cfg(not(feature = "integration-test"))]
                            {
                                button
                            }
                        })
                        .id(SharedString::from(format!("chat-jump-latest-{pane}")))
                        .debug_selector(|| "chat-jump-latest".into())
                        .role(Role::Button)
                        .aria_label("Jump to latest messages")
                        .occlude()
                        .flex()
                        .flex_col()
                        .items_center()
                        .gap(px(2.))
                        .px(px(12.))
                        .py(px(6.))
                        .rounded_full()
                        .border_1()
                        .border_color(rgb(theme.active))
                        .bg(rgb(theme.surface))
                        .text_color(rgb(ink))
                        .text_size(px((self.chat_text_size() - 1.).max(10.)))
                        .cursor_pointer()
                        .hover(|button| button.bg(rgb(theme.active)))
                        .child(
                            div()
                                .debug_selector(|| "chat-jump-label".into())
                                .child("Jump to latest"),
                        )
                        .child(
                            svg()
                                .debug_selector(|| "chat-jump-arrow".into())
                                .path("icons/arrow-down.svg")
                                .size(px(14.))
                                .text_color(rgb(ink)),
                        )
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                            cx.stop_propagation();
                            this.release_chat_text(&pane);
                            if let Some(chat) = this.chats.pane_mut(&pane) {
                                chat.selection.clear();
                                chat.glide.reset(&chat.list);
                                cx.notify();
                            }
                        })),
                )
                .into_any_element(),
        )
    }
}
