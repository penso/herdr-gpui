//! The chat as Apple's Messages draws it: the user's words in blue bubbles
//! on the right, the agent's in gray ones on the left, the last bubble of
//! each run ending in a tail toward its speaker, how a turn ended centered
//! between them as Messages dates a conversation, "Delivered" under a
//! prompt the agent has not answered yet, and "typing…" while the agent
//! works on a turn. The composer is a round field with the attach button
//! before it and send inside it.

use super::{super::column, ChatLayout, ComposerParts, Look, Row, Side, summary_row};
use crate::config::mix;
use gpui::{prelude::*, *};
use herdr_chat_protocol::EntryKind;
use unicode_width::UnicodeWidthStr;

/// Messages' own blues, for the user's bubbles on dark and light themes.
const BLUE_DARK: u32 = 0x0a_84_ff;
const BLUE_LIGHT: u32 = 0x00_7a_ff;
const WHITE: u32 = 0xff_ff_ff;
/// Space above a bubble where the speaker changes, and within a run.
const GAP: f32 = 14.;
const RUN_GAP: f32 = 2.;
/// Distances of Messages' tail at a 13pt text size, scaled with the chat's.
const TEXT: f32 = 13.;
const RADIUS: f32 = 17.;
const TAIL: f32 = 7.;

pub(super) struct IMessage;

impl ChatLayout for IMessage {
    fn row(&self, row: Row<'_>, look: &Look<'_>) -> AnyElement {
        let Some(side) = row.side() else {
            return summary_row(row, GAP);
        };
        let theme = look.theme;
        let id = row.entry.id.0;
        let tail = (row.next != Some(side)).then_some(side);
        let delivered = side == Side::User && row.next.is_none();
        let above = if row.speaker() { GAP } else { RUN_GAP };
        let small = px((look.text - 2.).max(10.));
        let body = match &row.entry.kind {
            // Pictures and notices stand alone, as Messages draws photos
            // and its own notes.
            EntryKind::Image { .. } => row.content,
            EntryKind::Notice { .. } => div()
                .flex()
                .justify_center()
                .text_size(small)
                .child(row.content),
            kind => {
                let (fill, text) = match side {
                    Side::User => (user_blue(look), WHITE),
                    Side::Agent => {
                        let share = if matches!(kind, EntryKind::Thought { .. }) {
                            8
                        } else {
                            14
                        };
                        let fill = mix(theme.background, theme.foreground, share);
                        (fill, theme.text_on(fill))
                    }
                };
                let bubble = bubble(row.content, fill, tail, look)
                    .debug_selector(move || format!("chat-imessage-{id}"))
                    .text_color(rgb(text))
                    .map(|bubble| {
                        // A reply still streaming takes the whole width, so
                        // it grows only downward rather than reflowing
                        // sideways with each longer line, and its text is
                        // not measured each frame it is drawn.
                        if row.live && matches!(kind, EntryKind::Agent { .. }) {
                            return bubble.w(relative(1.));
                        }
                        match message_width(kind, look) {
                            Some(width) => bubble.w(width),
                            None => bubble,
                        }
                    });
                div()
                    .flex()
                    .when(side == Side::User, |row| row.justify_end())
                    .child(bubble)
            }
        };
        column(
            div()
                .id(("chat-entry", id))
                .px(px(16.))
                .pt(px(above))
                .pb(px(2.))
                .min_w_0()
                .child(body)
                .children(delivered.then(|| {
                    div()
                        .debug_selector(|| "chat-delivered".into())
                        .flex()
                        .justify_end()
                        .pt(px(3.))
                        .pr(px(4.))
                        .text_size(small)
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(rgb(theme.muted))
                        .child("Delivered")
                })),
        )
        .into_any_element()
    }

    /// Floats over the message's lower corner, inside it, so the pointer
    /// can reach it without leaving the message.
    fn copy(&self, button: Stateful<Div>, look: &Look<'_>) -> Div {
        div().absolute().bottom_0().right_0().child(
            button
                .rounded_full()
                .bg(rgb(look.theme.background))
                .border_1()
                .border_color(rgb(look.theme.active)),
        )
    }

    /// Muted under the last bubble, where Messages shows that the other
    /// side is typing; its row stays, empty, between turns.
    fn working(&self, working: bool, look: &Look<'_>) -> Option<Div> {
        Some(
            div()
                .when(working, |row| row.debug_selector(|| "chat-typing".into()))
                .when(!working, |row| row.invisible())
                .px(px(16. + 12. * look.text / TEXT))
                .pb(px(4.))
                .text_size(px((look.text - 2.).max(10.)))
                .italic()
                .text_color(rgb(look.theme.muted))
                .child("typing\u{2026}"),
        )
    }

    fn turn(&self, text: &str, look: &Look<'_>) -> Div {
        div()
            .debug_selector(|| "chat-turn".into())
            .flex()
            .justify_center()
            .text_size(px((look.text - 2.).max(10.)))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(rgb(look.theme.muted))
            .child(text.to_owned())
    }

    fn composer(&self, parts: ComposerParts, look: &Look<'_>) -> Div {
        let theme = look.theme;
        let field = div()
            .flex_1()
            .min_w_0()
            .flex()
            .flex_col()
            .rounded(px(18.))
            .border_1()
            .border_color(rgb(mix(theme.background, theme.foreground, 22)))
            .children(parts.chips)
            .child(
                div()
                    .flex()
                    .items_end()
                    .gap(px(6.))
                    .pl(px(14.))
                    .pr(px(4.))
                    .py(px(3.))
                    .child(parts.draft.flex_1().min_w_0().py(px(3.)))
                    .child(parts.send),
            );
        div()
            .flex()
            .flex_col()
            .gap(px(6.))
            .child(
                div()
                    .flex()
                    .items_end()
                    .gap(px(8.))
                    .child(parts.attach.mb(px(3.)).bg(rgb(mix(
                        theme.background,
                        theme.foreground,
                        12,
                    ))))
                    .child(field),
            )
            .child(
                div()
                    .debug_selector(|| "chat-settings".into())
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap(px(6.))
                    .pl(px(36.))
                    .children(parts.settings)
                    .child(div().flex_1())
                    .children(parts.usage),
            )
    }
}

/// A width for an agent message's bubble, from its longest line. Its
/// Markdown lines wrap to whatever width they are given and claim none of
/// their own, so a bubble sized to its content would fold to a word per
/// line; the bubble's own `max_w` still caps a long one.
fn message_width(kind: &EntryKind, look: &Look<'_>) -> Option<Pixels> {
    let EntryKind::Agent { text } = kind else {
        return None;
    };
    let columns = text.lines().map(str::width).max().unwrap_or(0);
    let scale = look.text / TEXT;
    // An average proportional glyph, and room for the copy button.
    let text = columns.max(8) as f32 * look.text * 0.56;
    Some(px(text + 24. * scale))
}

fn user_blue(look: &Look<'_>) -> u32 {
    if crate::caption::dark(look.theme) {
        BLUE_DARK
    } else {
        BLUE_LIGHT
    }
}

/// `content` in a bubble of `fill`, with a tail toward `tail`'s side. The
/// bubble paints its own background so the tail can join it.
fn bubble(content: Div, fill: u32, tail: Option<Side>, look: &Look<'_>) -> Div {
    let scale = look.text / TEXT;
    let radius = px(RADIUS * scale);
    let overhang = px(TAIL * scale + 1.);
    let pages_under = look.pages_under();
    let color: Hsla = rgb(fill).into();
    // First, so the content paints over it; wider than the bubble by the
    // tail on each side, which only the tail paints into.
    let ground = canvas(
        |_, _, _| {},
        move |bounds, _, window, _| {
            let bubble = Bounds::from_corners(
                point(bounds.left() + overhang, bounds.top()),
                point(bounds.right() - overhang, bounds.bottom()),
            );
            // A page shown in the message lies beneath the window's drawing
            // on macOS, so the bubble leaves it showing, square.
            if let Some(pieces) = pages_under(bubble) {
                for piece in pieces {
                    window.paint_quad(gpui::fill(piece, color));
                }
                return;
            }
            match tail.map(|side| outline(bubble, side, scale)) {
                Some(Ok(path)) => window.paint_path(path, color),
                _ => window.paint_quad(gpui::fill(bubble, color).corner_radii(radius)),
            }
        },
    )
    .absolute()
    .top_0()
    .bottom_0()
    .left(-overhang)
    .right(-overhang);
    div()
        .relative()
        .max_w(relative(0.75))
        .min_w_0()
        .px(px(12. * scale))
        .py(px(6. * scale))
        .child(ground)
        .child(content.min_w_0())
}

/// The whole outline of a bubble with a tail: three rounded corners, and on
/// `side` the bubble's edge running down into a point that curls out past it,
/// its foot flat with the bubble's bottom. One path, so no seam shows where
/// the tail meets the bubble.
fn outline(bubble: Bounds<Pixels>, side: Side, scale: f32) -> Result<Path<Pixels>> {
    let (left, top, right, bottom) = (
        f32::from(bubble.left()),
        f32::from(bubble.top()),
        f32::from(bubble.right()),
        f32::from(bubble.bottom()),
    );
    let height = bottom - top;
    let r = (RADIUS * scale).min(height / 2.).min((right - left) / 2.);
    // How far up the edge the tail starts, and how far its point reaches.
    let rise = (11. * scale).min(height - r);
    let reach = TAIL * scale;
    let k = r * 0.552_284_8;
    // Drawn for a tail on the right; mirrored about the bubble's middle for
    // the left.
    let middle = (left + right) / 2.;
    let flip = side == Side::Agent;
    let at = |x: f32, y: f32| point(px(if flip { 2. * middle - x } else { x }), px(y));
    let mut path = PathBuilder::fill();
    path.move_to(at(left + r, top));
    path.line_to(at(right - r, top));
    path.cubic_bezier_to(
        at(right, top + r),
        at(right - r + k, top),
        at(right, top + r - k),
    );
    path.line_to(at(right, bottom - rise));
    // The point: hugging the edge, then sweeping out to a tip just above
    // the bottom, which rounds back into the flat foot.
    path.cubic_bezier_to(
        at(right + reach, bottom - 0.8 * scale),
        at(right, bottom - rise * 0.3),
        at(right + reach * 0.5, bottom - 0.6 * scale),
    );
    path.cubic_bezier_to(
        at(right - 2. * scale, bottom),
        at(right + reach - 1. * scale, bottom),
        at(right + 1. * scale, bottom),
    );
    path.line_to(at(left + r, bottom));
    path.cubic_bezier_to(
        at(left, bottom - r),
        at(left + r - k, bottom),
        at(left, bottom - r + k),
    );
    path.line_to(at(left, top + r));
    path.cubic_bezier_to(
        at(left + r, top),
        at(left, top + r - k),
        at(left + r - k, top),
    );
    path.close();
    path.build()
}
