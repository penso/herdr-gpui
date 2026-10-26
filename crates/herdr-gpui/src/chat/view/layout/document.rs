//! The chat as a document: each prompt has a speaker label and an accent bar,
//! the reply is prose under it with each tool call in a small framed box,
//! and a quiet rule under how the turn ended closes it. The composer is
//! one plain box with its settings along the foot.

use super::{ChatLayout, ComposerParts, Look, Row, Side, summary_row};
use crate::config::{corners, mix};
use gpui::{prelude::*, *};
use herdr_chat_protocol::EntryKind;

pub(super) struct Document;

impl ChatLayout for Document {
    fn row(&self, row: Row<'_>, look: &Look<'_>) -> AnyElement {
        let Some(side) = row.side() else {
            return summary_row(row, look, 10.);
        };
        let theme = look.theme;
        let speaker = row.speaker();
        let id = row.entry.id.0;
        let body = match (side, &row.entry.kind) {
            (Side::User, _) if speaker => div()
                .debug_selector(move || format!("chat-heading-{id}"))
                .mt(px(18.))
                .pl(px(10.))
                .border_l_2()
                .border_color(rgb(theme.primary()))
                .font_weight(FontWeight::NORMAL)
                .text_size(px(look.text))
                .child(
                    div()
                        .debug_selector(|| "chat-user-label".into())
                        .font_weight(FontWeight::SEMIBOLD)
                        .mb(px(6.))
                        .child("You"),
                )
                .child(row.content),
            (Side::User, _) => div()
                .mt(px(4.))
                .pl(px(12.))
                .font_weight(FontWeight::NORMAL)
                .text_size(px(look.text))
                .child(row.content),
            (Side::Agent, EntryKind::Tool(_)) => div().mt(px(6.)).child(
                div()
                    .px(px(8.))
                    .py(px(4.))
                    .rounded(px(corners::SMALL))
                    .border_1()
                    .border_color(rgb(theme.active))
                    .text_size(px((look.text - 1.).max(10.)))
                    .child(row.content),
            ),
            (Side::Agent, _) => div()
                .mt(px(if speaker { 12. } else { 8. }))
                .child(row.content),
        };
        look.column(
            div()
                .id(("chat-entry", id))
                .px(px(24.))
                .pb(px(2.))
                .min_w_0()
                .child(body),
        )
        .into_any_element()
    }

    fn turn(&self, text: AnyElement, look: &Look<'_>) -> Div {
        div()
            .debug_selector(|| "chat-turn".into())
            .flex()
            .flex_col()
            .gap(px(4.))
            .child(
                div()
                    .flex()
                    .justify_end()
                    .text_size(px((look.text - 1.).max(10.)))
                    .text_color(rgb(look.theme.muted))
                    .child(text),
            )
            .child(div().h(px(1.)).bg(rgb(look.theme.active)))
    }

    fn composer(&self, parts: ComposerParts, look: &Look<'_>) -> Div {
        let theme = look.theme;
        div()
            .flex()
            .flex_col()
            .gap(px(8.))
            .px(px(14.))
            .py(px(10.))
            .rounded(px(corners::CONTROL))
            .border_1()
            .border_color(rgb(mix(theme.background, theme.foreground, 24)))
            .children(parts.chips.map(|chips| chips.px_0().pt_0()))
            .child(parts.draft)
            .child(
                div()
                    .debug_selector(|| "chat-settings".into())
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap(px(8.))
                    .child(parts.attach)
                    .children(parts.settings)
                    .child(div().flex_1())
                    .children(parts.usage)
                    .child(parts.send),
            )
    }
}

#[cfg(test)]
mod tests;
