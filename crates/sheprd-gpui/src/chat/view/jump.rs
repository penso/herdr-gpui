//! A centered, animated way back to following the reply while reading earlier
//! entries. The same control says when new output is waiting below.

use super::*;
use crate::motion;

/// Twelve pixels above the entries, plus their eight-pixel bottom margin.
/// The entering button must remain above that margin throughout its rise.
const REST: f32 = 20.;
/// How far it rises into place as it appears, and sinks as it goes.
const RISE: f32 = 6.;

impl SheprdWindow {
    pub(super) fn render_chat_jump(&self, chat: &ChatPane, cx: &mut Context<Self>) -> Option<Div> {
        let shown = chat.glide.away_shown(motion::now())?;
        let theme = &self.theme;
        let ink = crate::contrast::ink_on_chrome(
            theme.palette[6],
            [theme.surface, theme.active, theme.surface],
            4.5,
        );
        let pane = chat.target.pane_id.clone();
        let waiting = chat.glide.waiting();
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
                        .flex_col()
                        .items_center()
                        .justify_center()
                        .gap(px(2.))
                        .px(px(12.))
                        .py(px(6.))
                        .rounded_full()
                        .border_1()
                        .border_color(rgb(theme.active))
                        .bg(rgb(theme.surface))
                        .cursor_pointer()
                        .hover(|button| button.bg(rgb(theme.active)))
                        .child(
                            div()
                                .debug_selector(|| "chat-jump-label".into())
                                .text_size(self.chat_small_text())
                                .text_color(rgb(ink))
                                .map(|label| {
                                    if waiting {
                                        label.child(motion::pop_in(
                                            div()
                                                .debug_selector(|| "chat-jump-waiting".into())
                                                .child(fl!("chat-jump-waiting")),
                                            SharedString::from(format!("chat-jump-waiting-{pane}")),
                                        ))
                                    } else {
                                        label.child(fl!("rich-reading-jump"))
                                    }
                                }),
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
                            this.jump_chat_to_end(&pane, cx);
                        })),
                ),
        )
    }

    fn jump_chat_to_end(&mut self, pane_id: &str, cx: &mut Context<Self>) {
        self.release_chat_text(pane_id);
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
