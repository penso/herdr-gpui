//! The user's messages in bubbles on the right, the agent's as plain text
//! across the column, and no names or avatars, as Claude's app draws a
//! conversation. The composer is the draft alone in a box, with the
//! settings in a quiet row under it.

use super::{
    Bubble, ChatLayout, ComposerParts, Look, Row, Side, ToolIndent, bubble_fill, bubble_row,
    summary_row,
};
use crate::config::{corners, mix};
use gpui::{prelude::*, *};

const SHAPE: Bubble = Bubble {
    radius: 10.,
    padding: 7.,
    gap: 18.,
};

pub(super) struct Bubbles;

impl ChatLayout for Bubbles {
    fn row(&self, row: Row<'_>, look: &Look<'_>) -> AnyElement {
        if row.side().is_none() {
            return summary_row(row, look, SHAPE.gap);
        }
        bubble_row(row, look, SHAPE)
    }

    fn ground(&self, side: Side, look: &Look<'_>) -> Option<u32> {
        Some(match side {
            Side::User => bubble_fill(look),
            Side::Agent => look.theme.background,
        })
    }

    fn tool_indent(&self) -> Option<ToolIndent> {
        // The body sits past the header so the two read as one nesting
        // level; the prose keeps the column edge.
        Some(ToolIndent {
            header: 10.,
            body: 28.,
        })
    }

    fn composer(&self, parts: ComposerParts, look: &Look<'_>) -> Div {
        let theme = look.theme;
        let field = div()
            .flex()
            .flex_col()
            .rounded(px(corners::PANEL + 4.))
            .border_1()
            .border_color(rgb(mix(theme.background, theme.foreground, 24)))
            .bg(rgb(mix(theme.background, theme.foreground, 4)))
            .children(parts.chips)
            .child(
                div()
                    .flex()
                    .items_end()
                    .gap(px(8.))
                    .pl(px(14.))
                    .pr(px(8.))
                    .py(px(8.))
                    .child(parts.draft.flex_1().min_w_0().py(px(2.)))
                    .child(parts.send),
            );
        div().flex().flex_col().gap(px(6.)).child(field).child(
            div()
                .debug_selector(|| "chat-settings".into())
                .flex()
                .flex_wrap()
                .items_center()
                .gap(px(6.))
                .px(px(4.))
                .child(parts.attach)
                .children(parts.settings)
                .child(div().flex_1())
                .children(parts.usage),
        )
    }
}
