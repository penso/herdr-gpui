//! The chat as Claude Code draws its own screen: the terminal's font
//! throughout, each prompt on a shaded line after `›`, the agent's words
//! and tool calls after `⏺` colored by state, how a turn ended after `✻`,
//! and the session's settings in a status line under the prompt box.

use super::{super::Hues, ChatLayout, ComposerParts, Look, Row, Side, summary_row};
use crate::{
    config::{corners, mix},
    fonts::StyledFont,
};
use gpui::{prelude::*, *};
use herdr_chat_protocol::{EntryKind, NoticeLevel, ToolStatus};

pub(super) struct Terminal;

/// The column a marker takes, in widths of the terminal's font size.
fn gutter(look: &Look<'_>) -> Pixels {
    px(look.mono.size * 1.6)
}

impl ChatLayout for Terminal {
    fn row(&self, row: Row<'_>, look: &Look<'_>) -> AnyElement {
        let Some(side) = row.side() else {
            return summary_row(row, look, 6.);
        };
        let theme = look.theme;
        let hues = Hues::of(theme);
        let speaker = row.speaker();
        let id = row.entry.id.0;
        let (mark, color) = match (&row.entry.kind, side) {
            (_, Side::User) => ("\u{203a}", theme.muted),
            (EntryKind::Tool(tool), _) => (
                "\u{23fa}",
                match tool.status {
                    ToolStatus::Pending => theme.muted,
                    ToolStatus::InProgress => hues.yellow,
                    ToolStatus::Completed => hues.green,
                    ToolStatus::Failed => hues.red,
                },
            ),
            (EntryKind::Notice { level, .. }, _) => (
                "\u{23fa}",
                match level {
                    NoticeLevel::Info => theme.subtext(),
                    NoticeLevel::Warning => hues.yellow,
                    NoticeLevel::Error => hues.red,
                },
            ),
            (EntryKind::Thought { .. } | EntryKind::Image { .. }, _) => ("\u{23fa}", theme.muted),
            _ => ("\u{23fa}", theme.foreground),
        };
        let marker = div()
            .debug_selector(move || format!("chat-mark-{id}"))
            .flex_none()
            .w(gutter(look))
            .text_color(rgb(color))
            .child(mark);
        let line = div()
            .flex()
            .min_w_0()
            .child(marker)
            .child(row.content.flex_1().min_w_0());
        let line = match side {
            Side::User => line
                .mt(px(if speaker { 14. } else { 2. }))
                .px(px(8.))
                .py(px(4.))
                .bg(rgb(mix(theme.background, theme.foreground, 7)))
                .text_color(rgb(theme.subtext())),
            Side::Agent => line.mt(px(if speaker { 10. } else { 6. })),
        };
        look.column(
            div()
                .id(("chat-entry", id))
                .px(px(14.))
                .pb(px(2.))
                .text_font(look.mono)
                .text_size(px(look.mono.size))
                .child(line),
        )
        .into_any_element()
    }

    fn turn(&self, text: AnyElement, look: &Look<'_>) -> Div {
        div()
            .debug_selector(|| "chat-turn".into())
            .pl(gutter(look))
            .text_font(look.mono)
            .text_size(px((look.mono.size - 1.).max(10.)))
            .text_color(rgb(look.theme.muted))
            .flex()
            .gap(px(5.))
            .child("\u{273b}")
            .child(text)
    }

    fn composer(&self, parts: ComposerParts, look: &Look<'_>) -> Div {
        let theme = look.theme;
        div()
            .flex()
            .flex_col()
            .gap(px(4.))
            .text_font(look.mono)
            .children(parts.chips)
            .child(
                div()
                    .flex()
                    .items_start()
                    .px(px(8.))
                    .py(px(6.))
                    .rounded(px(corners::SMALL))
                    .border_1()
                    .border_color(rgb(mix(theme.background, theme.foreground, 30)))
                    .child(
                        div()
                            .flex_none()
                            .w(gutter(look))
                            .pt(px(2.))
                            .text_color(rgb(theme.foreground))
                            .child(">"),
                    )
                    .child(parts.draft.flex_1().min_w_0()),
            )
            .child(
                div()
                    .debug_selector(|| "chat-settings".into())
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap(px(6.))
                    .px(px(2.))
                    .text_size(px((look.mono.size - 1.).max(10.)))
                    .text_color(rgb(theme.muted))
                    .child(parts.attach)
                    .children(parts.settings)
                    .child(div().flex_1())
                    .children(parts.usage)
                    .child(parts.send),
            )
    }
}
