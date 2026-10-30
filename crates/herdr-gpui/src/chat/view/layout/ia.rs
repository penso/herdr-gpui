//! The chat as iA Writer sets text: one column at a book's measure, the
//! user's prompts and the agent's steps as quiet notes hung from a gutter,
//! the agent's prose in full ink, and everything before the current turn
//! muted. Pointing at an earlier entry brings it back to full ink.

use super::{ChatLayout, ComposerParts, Look, Row, Side};
use crate::config::mix;
use gpui::{prelude::*, *};
use herdr_chat_protocol::{Author, EntryKind};

/// Width of the gutter a prompt's mark hangs in, in ems of the chat's text.
const GUTTER: f32 = 1.6;
/// How much of an entry before the current turn shows.
const QUIET: f32 = 0.4;
/// How many points smaller than the prose a note is set.
const NOTE_STEP: f32 = 3.;

pub(super) struct IA;

/// Whether an entry frames the answer, quieter and smaller, or is the
/// answer itself.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Place {
    Note,
    Prose,
}

fn place(kind: &EntryKind) -> Place {
    match kind {
        EntryKind::User { .. }
        | EntryKind::Tool(_)
        | EntryKind::Thought { .. }
        | EntryKind::Turn(_)
        | EntryKind::Image {
            author: Author::User,
            ..
        } => Place::Note,
        _ => Place::Prose,
    }
}

/// The text size of a note.
fn note_text(look: &Look<'_>) -> f32 {
    (look.text - NOTE_STEP).max(10.)
}

impl ChatLayout for IA {
    fn row(&self, row: Row<'_>, look: &Look<'_>) -> AnyElement {
        let theme = look.theme;
        let em = look.text;
        let id = row.entry.id.0;
        let place = place(&row.entry.kind);
        let prompt = matches!(row.entry.kind, EntryKind::User { .. });
        // A prompt opens a turn, so it keeps its distance from the last one.
        let above = if prompt && row.speaker() && row.previous.is_some() {
            em * 1.8
        } else if row.side() == Some(Side::Agent) && place == Place::Prose {
            em * 0.6
        } else {
            em * 0.3
        };
        let ink = match place {
            Place::Note if prompt => theme.subtext(),
            Place::Note => theme.muted,
            Place::Prose => theme.foreground,
        };
        let gutter = px(em * GUTTER);
        let body = div()
            .relative()
            .pl(gutter)
            .when(prompt, |row| {
                row.child(
                    div()
                        .debug_selector(move || format!("chat-ia-mark-{id}"))
                        .absolute()
                        .left_0()
                        .top_0()
                        .w(gutter)
                        .pr(px(6.))
                        .flex()
                        .justify_end()
                        .text_color(rgb(theme.muted))
                        .child("\u{203a}"),
                )
            })
            .child(
                div()
                    .min_w_0()
                    .text_color(rgb(ink))
                    .when(place == Place::Note, |note| {
                        note.text_size(px(note_text(look)))
                            .line_height(relative(1.45))
                    })
                    .when(prompt, |note| note.italic())
                    .child(row.content),
            );
        look.column(
            div()
                .id(("chat-entry", id))
                .debug_selector(move || format!("chat-ia-{id}"))
                .px(px(16.))
                .pt(px(above))
                .min_w_0()
                // Before the current turn the text steps back, and comes
                // forward again while the pointer rests on it.
                .when(!row.current, |row| {
                    row.opacity(QUIET).hover(|row| row.opacity(1.))
                })
                .child(body),
        )
        .into_any_element()
    }

    fn prompt_text(&self, look: &Look<'_>) -> f32 {
        note_text(look)
    }

    /// A step in the margin is one line until it is opened.
    fn tool_preview(&self) -> bool {
        false
    }

    /// Each row sets its own ink, so a note in the margin reads quieter
    /// than the prose in the column.
    fn prose(&self, _look: &Look<'_>) -> Option<u32> {
        None
    }

    fn turn(&self, text: AnyElement, look: &Look<'_>) -> Div {
        div()
            .debug_selector(|| "chat-turn".into())
            .text_size(px(note_text(look)))
            .text_color(rgb(look.theme.muted))
            .child(text)
    }

    fn composer(&self, parts: ComposerParts, look: &Look<'_>) -> Div {
        let theme = look.theme;
        div()
            .flex()
            .flex_col()
            .gap(px(8.))
            .px(px(16.))
            .pt(px(12.))
            .pb(px(8.))
            .border_t_1()
            .border_color(rgb(mix(theme.background, theme.foreground, 14)))
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
