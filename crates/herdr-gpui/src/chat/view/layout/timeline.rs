//! Entries on a timeline: a gutter down the left marks each entry, with an
//! avatar where the speaker changes and a dot colored by state for the
//! agent's thoughts, tool calls, and notices. The line through the gutter
//! breaks at each turn's summary. The composer is one card, filled a
//! shade off the background with no border, its settings along the foot.

use super::{
    super::{Hues, agent_icon},
    ChatLayout, ComposerParts, Look, Row, Side, side, summary_row,
};
use crate::{config::corners, notifications::safe_text};
use gpui::{prelude::*, *};
use herdr_chat_protocol::{Entry, EntryKind, NoticeLevel, ToolStatus};

/// Width of the timeline's gutter, and the size of an avatar in it.
const GUTTER: f32 = 22.;
/// Space above each entry, which the gutter's line spans.
const SPACING: f32 = 8.;

pub(super) struct Timeline;

impl ChatLayout for Timeline {
    fn row(&self, row: Row<'_>, look: &Look<'_>) -> AnyElement {
        let Some(this_side) = row.side() else {
            return summary_row(row, look, SPACING + 4.);
        };
        let speaker = row.speaker();
        let theme = look.theme;
        let line = rgb(crate::config::mix(theme.background, theme.foreground, 22));
        let gutter = div()
            .flex_none()
            .w(px(GUTTER))
            .flex()
            .flex_col()
            .items_center()
            .child(
                div()
                    .w(px(1.))
                    .h(px(SPACING))
                    .when(row.previous.is_some(), |segment| segment.bg(line)),
            )
            .child(mark(row.entry, speaker, row.agent, look))
            .child(
                div()
                    .flex_1()
                    .w(px(1.))
                    .when(row.next.is_some(), |segment| segment.bg(line)),
            );
        let label = speaker.then(|| {
            let name = match this_side {
                Side::User => "You".to_owned(),
                Side::Agent => safe_text(row.agent, 80),
            };
            div()
                .debug_selector(|| "chat-speaker".into())
                .font_weight(FontWeight::SEMIBOLD)
                .child(name)
        });
        look.column(
            div()
                .id(("chat-entry", row.entry.id.0))
                .flex()
                .px(px(12.))
                .min_w_0()
                .child(gutter)
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .gap(px(2.))
                        .pl(px(10.))
                        .pt(px(SPACING))
                        .pb(px(2.))
                        .children(label)
                        .child(row.content),
                ),
        )
        .into_any_element()
    }

    fn composer(&self, parts: ComposerParts, look: &Look<'_>) -> Div {
        let theme = look.theme;
        div()
            .flex()
            .flex_col()
            .rounded(px(corners::PANEL))
            .bg(rgb(crate::config::mix(
                theme.background,
                theme.foreground,
                5,
            )))
            .children(parts.chips)
            .child(parts.draft.px(px(14.)).pt(px(12.)).pb(px(8.)))
            .child(
                div()
                    .debug_selector(|| "chat-settings".into())
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap(px(8.))
                    .px(px(10.))
                    .pb(px(10.))
                    .child(parts.attach)
                    .children(parts.settings)
                    .child(div().flex_1())
                    .children(parts.usage)
                    .child(parts.send),
            )
    }
}

/// The gutter's mark for an entry: an avatar where the speaker changes,
/// else a dot colored by the entry's state.
fn mark(entry: &Entry, speaker: bool, agent: &str, look: &Look<'_>) -> Div {
    let theme = look.theme;
    let hues = Hues::of(theme);
    if speaker {
        let (icon, fill) = match side(&entry.kind) {
            Some(Side::User) => ("icons/user.svg", theme.active),
            _ => (agent_icon(agent), theme.surface),
        };
        let id = entry.id.0;
        return div()
            .debug_selector(move || format!("chat-avatar-{id}"))
            .flex_none()
            .size(px(GUTTER))
            .flex()
            .items_center()
            .justify_center()
            .rounded_full()
            .border_1()
            .border_color(rgb(theme.active))
            .bg(rgb(fill))
            .child(
                svg()
                    .path(icon)
                    .size(px(12.))
                    .text_color(rgb(theme.text_on(fill))),
            );
    }
    let (color, filled) = match &entry.kind {
        EntryKind::Tool(tool) => match tool.status {
            ToolStatus::Pending => (theme.muted, false),
            ToolStatus::InProgress => (hues.yellow, false),
            ToolStatus::Completed => (hues.green, true),
            ToolStatus::Failed => (hues.red, true),
        },
        EntryKind::Notice { level, .. } => match level {
            NoticeLevel::Info => (theme.subtext(), false),
            NoticeLevel::Warning => (hues.yellow, true),
            NoticeLevel::Error => (hues.red, true),
        },
        _ => (theme.muted, false),
    };
    // A row as tall as a line of text, so the dot sits beside the first.
    let id = entry.id.0;
    div()
        .debug_selector(move || format!("chat-dot-{id}"))
        .flex_none()
        .h(px(look.text * 1.5))
        .flex()
        .items_center()
        .child(
            div()
                .size(px(8.))
                .rounded_full()
                .border_2()
                .border_color(rgb(color))
                .bg(rgb(if filled { color } else { theme.background })),
        )
}
