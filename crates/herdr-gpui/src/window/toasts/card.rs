//! One toast's card: its host, badge, title, body, and dismiss button.

use super::{
    badge,
    motion::{Drawn, offset},
};
use crate::{HerdrWindow, fonts::StyledFont, notifications::safe_text};
use gpui::{prelude::*, *};
use herdr_client::protocol::SemanticNotificationKind;

/// How a card is drawn: in place, or as the copy of one sliding out.
pub(super) struct Look {
    /// From 0 to 1, how much of it shows.
    pub(super) shown: f32,
    /// Whether it answers the pointer. A toast on its way out has already
    /// left the queue, so it takes no input.
    pub(super) live: bool,
    /// Whether a click opens what it is about.
    pub(super) navigable: bool,
    pub(super) max_height: Pixels,
    pub(super) body_room: Pixels,
}

impl HerdrWindow {
    pub(super) fn toast_card(
        &self,
        toast: &Drawn,
        look: Look,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let notice = &toast.notice;
        let endpoint_id = toast.key.endpoint.clone();
        let generation = toast.key.generation;
        let id = toast.key.id;
        let badge = badge::badge(notice);
        let accent = self.theme.ink(match notice.kind {
            SemanticNotificationKind::NeedsAttention => self.theme.palette[3],
            SemanticNotificationKind::Finished => self.theme.palette[2],
            _ => self.theme.primary(),
        });
        div()
            .id(SharedString::from(format!(
                "toast-{endpoint_id}-{generation}-{id}{}",
                if look.live { "" } else { "-leaving" }
            )))
            .when(!look.live, |card| {
                card.debug_selector({
                    let endpoint_id = endpoint_id.clone();
                    move || format!("toast-leaving-{endpoint_id}-{id}")
                })
            })
            .when(look.live, |card| {
                card.debug_selector({
                    let endpoint_id = endpoint_id.clone();
                    move || format!("toast-{endpoint_id}-{id}")
                })
                .occlude()
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .on_mouse_down(MouseButton::Right, |_, _, cx| cx.stop_propagation())
                .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
                .when(look.navigable, |d| d.cursor_pointer())
                .on_click({
                    let endpoint_id = endpoint_id.clone();
                    let inbox = toast.inbox.clone();
                    cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        this.click_toast(&endpoint_id, generation, &inbox, id, cx);
                    })
                })
            })
            .relative()
            .left(px(offset(toast.corner, look.shown)))
            .opacity(look.shown)
            .w_full()
            .flex_none()
            .overflow_hidden()
            .max_h(look.max_height)
            .rounded(px(crate::config::corners::PANEL))
            .border_1()
            .border_color(rgb(crate::config::mix(
                self.theme.surface,
                self.theme.foreground,
                14,
            )))
            .bg(rgb(self.theme.surface))
            .text_color(rgb(self.theme.foreground))
            .text_font(&self.config.ui)
            .text_size(px(self.config.ui.size))
            .p(px(10.))
            .flex()
            .gap(px(10.))
            .child(
                badge::circle(badge.mark, &self.theme, accent).debug_selector({
                    let endpoint_id = endpoint_id.clone();
                    move || format!("toast-badge-{endpoint_id}-{id}")
                }),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .flex()
                    .flex_col()
                    .gap(px(4.))
                    .child(
                        div()
                            .truncate()
                            .text_color(rgb(self.theme.muted))
                            .child(safe_text(&toast.label, 80)),
                    )
                    .child({
                        let title = SharedString::from(badge.title.to_owned());
                        div()
                            // Names the drawn text, so tests can tell which
                            // title reached the screen.
                            .debug_selector({
                                let endpoint_id = endpoint_id.clone();
                                let title = title.clone();
                                move || format!("toast-title-{endpoint_id}-{id}: {title}")
                            })
                            .truncate()
                            .font_weight(FontWeight::MEDIUM)
                            .child(title)
                    })
                    .when_some(notice.body.clone(), |d, body| {
                        d.child(
                            div()
                                .id("body")
                                .debug_selector({
                                    let endpoint_id = endpoint_id.clone();
                                    move || format!("toast-body-{endpoint_id}-{id}")
                                })
                                .when(notice.is_local_feedback(), |body| {
                                    body.max_h(look.body_room).overflow_y_scroll()
                                })
                                .when(!notice.is_local_feedback(), |body| {
                                    body.max_h(px(48.)).overflow_hidden()
                                })
                                // A long task must not wrap the place out of
                                // the clipped body.
                                .when(notice.described, |d| {
                                    d.children(body.lines().enumerate().map(|(row, line)| {
                                        let endpoint_id = endpoint_id.clone();
                                        div()
                                            .debug_selector(move || {
                                                format!("toast-body-row-{endpoint_id}-{id}-{row}")
                                            })
                                            .truncate()
                                            .child(SharedString::from(line.to_owned()))
                                    }))
                                })
                                .when(!notice.described, |d| d.child(body)),
                        )
                    }),
            )
            .child(
                div()
                    .id("dismiss")
                    .debug_selector({
                        let endpoint_id = endpoint_id.clone();
                        move || format!("toast-dismiss-{endpoint_id}-{id}")
                    })
                    .flex_none()
                    .size(px(24.))
                    .rounded(px(crate::config::corners::CONTROL))
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(
                        svg()
                            .path("icons/close.svg")
                            .size(px(12.))
                            .text_color(rgb(self.theme.foreground)),
                    )
                    .when(look.live, |button| {
                        let inbox = toast.inbox.clone();
                        button
                            .cursor_pointer()
                            .hover(|s| s.bg(rgb(self.theme.active)))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                cx.stop_propagation();
                                if let Some(endpoint) = this.endpoints.iter_mut().find(|e| {
                                    e.id == endpoint_id
                                        && e.generation == generation
                                        && std::sync::Arc::ptr_eq(&e.connection.inbox, &inbox)
                                }) {
                                    endpoint.toasts.dismiss(id);
                                    cx.notify();
                                }
                            }))
                    }),
            )
    }
}
