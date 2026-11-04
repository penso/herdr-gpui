//! Each turn in one card: the prompt on top after the user's mark, the
//! reply under a hairline with the agent's logo, and how the turn ended at
//! its foot. A card spans several rows of the list: the prompt's row opens
//! it, the rows inside carry its sides, and the turn's summary closes it,
//! so the rows abut with no space between them.

use super::{super::agent_icon, ChatLayout, ComposerParts, Look, Row, Side};
use crate::config::{corners, mix};
use gpui::{prelude::*, *};

pub(super) struct Cards;

/// Space between cards.
const GAP: f32 = 12.;

impl ChatLayout for Cards {
    fn row(&self, row: Row<'_>, look: &Look<'_>) -> AnyElement {
        let theme = look.theme;
        let id = row.entry.id.0;
        let side = row.side();
        // A card opens at a prompt, or at whatever follows a closed card.
        let opens = match side {
            Some(Side::User) => row.speaker(),
            _ => row.previous.is_none(),
        };
        let closes = side.is_none();
        let speaker = row.speaker();
        let inner = match side {
            Some(Side::User) if speaker => div()
                .flex()
                .gap(px(8.))
                .px(px(12.))
                .py(px(8.))
                .font_weight(FontWeight::SEMIBOLD)
                .child(
                    svg()
                        .flex_none()
                        .mt(px(3.))
                        .path("icons/user.svg")
                        .size(px(12.))
                        .text_color(rgb(theme.subtext())),
                )
                .child(row.content.flex_1().min_w_0()),
            Some(Side::User) => div().px(px(12.)).pl(px(32.)).pb(px(8.)).child(row.content),
            Some(Side::Agent) => div()
                .flex()
                .gap(px(10.))
                .px(px(12.))
                .pt(px(if speaker { 10. } else { 4. }))
                .pb(px(4.))
                .child(div().flex_none().w(px(14.)).children(speaker.then(|| {
                    svg()
                        .mt(px(3.))
                        .path(agent_icon(row.agent))
                        .size(px(14.))
                        .text_color(rgb(theme.foreground))
                })))
                .child(row.content.flex_1().min_w_0()),
            None => div().px(px(12.)).pt(px(4.)).pb(px(8.)).child(row.content),
        };
        // The prompt's hairline: under the last of the user's rows.
        let inner = inner.when(
            side == Some(Side::User) && row.next == Some(Side::Agent),
            |inner| inner.border_b_1().border_color(rgb(theme.active)),
        );
        let piece = div()
            .debug_selector(move || format!("chat-card-{id}"))
            .min_w_0()
            .border_l_1()
            .border_r_1()
            .border_color(rgb(theme.active))
            .when(opens, |piece| {
                piece.border_t_1().rounded_t(px(corners::PANEL))
            })
            .when(closes, |piece| {
                piece.border_b_1().rounded_b(px(corners::PANEL))
            })
            .child(inner);
        // A page shown in the card lies beneath its background on macOS.
        let piece = look.ground(piece, mix(theme.background, theme.foreground, 3));
        look.column(
            div()
                .id(("chat-entry", id))
                .px(px(16.))
                .when(opens, |row| row.pt(px(GAP)))
                .child(piece),
        )
        .into_any_element()
    }

    fn turn(&self, text: AnyElement, look: &Look<'_>) -> Div {
        div()
            .debug_selector(|| "chat-turn".into())
            .flex()
            .justify_end()
            .text_size(px((look.text - 1.).max(10.)))
            .text_color(rgb(look.theme.muted))
            .child(text)
    }

    fn composer(&self, parts: ComposerParts, look: &Look<'_>) -> Div {
        let theme = look.theme;
        div()
            .flex()
            .flex_col()
            .rounded(px(corners::PANEL))
            .border_1()
            .border_color(rgb(mix(theme.background, theme.foreground, 24)))
            .bg(rgb(mix(theme.background, theme.foreground, 3)))
            .children(parts.chips)
            .child(parts.draft.px(px(12.)).pt(px(10.)).pb(px(6.)))
            .child(
                div()
                    .debug_selector(|| "chat-settings".into())
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap(px(8.))
                    .px(px(8.))
                    .pb(px(8.))
                    .child(parts.attach)
                    .children(parts.settings)
                    .child(div().flex_1())
                    .children(parts.usage)
                    .child(parts.send),
            )
    }
}
