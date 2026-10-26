//! An agent's compaction summary as one quiet card: its gist folded, its
//! non-empty sections when opened.

use super::{
    super::{
        selection::{Cell, Region},
        summary::Summary,
    },
    disclosure::HeaderText,
};
use crate::{HerdrWindow, config::corners};
use gpui::{prelude::*, *};
use herdr_chat_protocol::Entry;

impl HerdrWindow {
    pub(super) fn render_chat_summary(
        &self,
        pane_id: &str,
        entry: &Entry,
        summary: &Summary,
        expanded: bool,
        cx: &mut Context<Self>,
    ) -> Div {
        let theme = &self.theme;
        let small = self.chat_small_text();
        let id = entry.id;
        let card = div()
            .id(("chat-summary", id.0))
            .debug_selector(move || format!("chat-summary-{}", id.0))
            .flex()
            .flex_col()
            .gap(px(6.))
            .px(px(10.))
            .py(px(8.))
            .rounded(px(corners::CONTROL))
            .bg(rgb(theme.surface))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .text_size(small)
                    .text_color(rgb(theme.subtext()))
                    .child(
                        svg()
                            .flex_none()
                            .path("icons/note.svg")
                            .size(px(11.))
                            .text_color(rgb(theme.subtext())),
                    )
                    .child("Context summary")
                    .child(div().flex_1())
                    .child(
                        div()
                            .flex_none()
                            .text_color(rgb(theme.primary()))
                            .child(if expanded { "Hide" } else { "Show" }),
                    ),
            )
            .map(|card| {
                if expanded {
                    card.child(self.render_chat_lines(
                        "chat-summary",
                        self.chat_text_scope(pane_id, entry.id, Region::Body(0)),
                        &summary.lines,
                        cx,
                    ))
                } else {
                    card.child(div().min_w_0().truncate().text_size(small).child(
                        self.render_chat_text(
                            pane_id,
                            entry.id,
                            Region::Body(0),
                            Cell::default(),
                            summary.gist.clone(),
                            cx,
                        ),
                    ))
                }
            });
        div().child(self.chat_disclosure(card, pane_id, id, expanded, HeaderText::Body, cx))
    }
}
