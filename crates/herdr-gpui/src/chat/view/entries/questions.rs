//! Answered questions are readable history, with copy and source disclosure.
//! This card creates no input controls; unanswered questions stay with the agent.

use super::*;

impl HerdrWindow {
    pub(super) fn render_chat_answers(
        &self,
        pane_id: &str,
        entry: &Entry,
        cx: &mut Context<Self>,
    ) -> Div {
        let EntryKind::Notice {
            details,
            level,
            text: label,
            ..
        } = &entry.kind
        else {
            return div();
        };
        let Some(chat) = self.chats.pane(pane_id) else {
            return div();
        };
        let body = chat.prepared.body(entry.id);
        let Some(Body::Answers { text, parts }) = body.as_deref() else {
            return div().child("Preparing answers…");
        };
        let message =
            self.render_chat_message(pane_id, entry.id, parts, &chat.prepared.visuals, cx);
        let id = entry.id.0;
        div()
            .debug_selector(move || format!("chat-answers-{id}"))
            .flex()
            .flex_col()
            .gap(px(8.))
            .min_w_0()
            .child(
                div()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(self.render_chat_text(
                        pane_id,
                        entry.id,
                        Region::Label,
                        Cell::default(),
                        notice_label(label),
                        cx,
                    )),
            )
            .child(message)
            .child(div().child(self.rich_copy(super::super::copy::ANSWERS, text.clone(), cx)))
            .when(details.is_some(), |card| {
                card.child(self.render_chat_notice_record(
                    pane_id,
                    entry.id,
                    *level,
                    "Original record",
                    details.as_deref(),
                    cx,
                ))
            })
    }
}
