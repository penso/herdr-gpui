//! The round button over the entries once the list is left above its end,
//! which goes back to the end and to following the reply. Once output lands
//! below, it grows into a pill saying so.

use super::*;
use crate::motion;

/// How far above the bottom of the entries the button rests.
const REST: f32 = 12.;
/// How far it rises into place as it appears, and sinks as it goes.
const RISE: f32 = 6.;
const SIZE: f32 = 28.;

impl SheprdWindow {
    pub(super) fn render_chat_jump(&self, chat: &ChatPane, cx: &mut Context<Self>) -> Option<Div> {
        let shown = chat.glide.away_shown(motion::now())?;
        let theme = &self.theme;
        let pane = chat.target.pane_id.clone();
        let waiting = chat.glide.waiting();
        let labelled = layout::layout_for(self.config.chat.layout).jump_label();
        Some(
            div()
                .absolute()
                .left_0()
                .right_0()
                .bottom(px(REST - (1. - shown) * RISE))
                .flex()
                .justify_center()
                .opacity(shown)
                .child(
                    div()
                        .id(SharedString::from(format!("chat-jump-{pane}")))
                        .debug_selector(|| "chat-jump".into())
                        .role(Role::Button)
                        .aria_label(if waiting {
                            fl!("chat-jump-waiting")
                        } else {
                            fl!("chat-jump-to-end")
                        })
                        .occlude()
                        .flex()
                        .items_center()
                        .justify_center()
                        .gap(px(6.))
                        .h(px(SIZE))
                        .min_w(px(SIZE))
                        .when(waiting || labelled, |button| button.px(px(12.)))
                        .rounded_full()
                        .border_1()
                        .border_color(rgb(theme.active))
                        .bg(rgb(theme.surface))
                        .cursor_pointer()
                        .hover(|button| button.bg(rgb(theme.active)))
                        .when(waiting, |button| {
                            button.child(motion::pop_in(
                                div()
                                    .debug_selector(|| "chat-jump-waiting".into())
                                    .text_size(self.chat_small_text())
                                    .text_color(rgb(theme.foreground))
                                    .child(fl!("chat-jump-waiting")),
                                SharedString::from(format!("chat-jump-waiting-{pane}")),
                            ))
                        })
                        .when(labelled && !waiting, |button| {
                            button.child(
                                div()
                                    .debug_selector(|| "chat-jump-label".into())
                                    .text_size(self.chat_small_text())
                                    .text_color(rgb(theme.foreground))
                                    .child(fl!("chat-jump-to-end")),
                            )
                        })
                        .child(
                            svg()
                                .path("icons/chevron-down.svg")
                                .size(px(14.))
                                .text_color(rgb(theme.foreground)),
                        )
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                            cx.stop_propagation();
                            this.jump_chat_to_end(&pane, cx);
                        })),
                ),
        )
    }

    fn jump_chat_to_end(&mut self, pane_id: &str, cx: &mut Context<Self>) {
        if let Some(pane) = self.chats.pane_mut(pane_id) {
            // A selection holds the list, which could not follow from here.
            pane.selection.clear();
            pane.text_drag = Default::default();
            pane.glide.reset(&pane.list);
        }
        self.redraw_terminal(cx);
        cx.notify();
    }
}
