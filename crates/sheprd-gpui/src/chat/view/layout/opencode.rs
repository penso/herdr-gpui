//! The chat as OpenCode draws its own screen: the terminal's font
//! throughout, prompts and open tool calls as tinted blocks behind a bar on
//! the left, a finished call folded to one line, the session's settings
//! inside the composer's block, and a status row under it.

use super::{
    super::{Hues, entries::ToolState},
    ChatLayout, ComposerParts, Look, Row, Side, ToolFold, summary_row,
};
use crate::{
    config::{Theme, mix},
    fonts::StyledFont,
};
use gpui::{prelude::*, *};

#[cfg(test)]
mod tests;

pub(super) struct OpenCode;

/// How far a block's text sits from its bar, which is also where the rows
/// without a block start, so all text shares one edge.
const INSET: f32 = 20.;
const BAR: f32 = 3.;
/// How much of the foreground tints each kind of block.
const PROMPT_TINT: u32 = 10;
const TOOL_TINT: u32 = 7;
const RUNNING_TINT: u32 = 12;
const COMPOSER_TINT: u32 = 12;
const BAR_TINT: u32 = 22;

fn tint(theme: &Theme, percent: u32) -> u32 {
    mix(theme.background, theme.foreground, percent)
}

/// The bright red, held to a mark's contrast on its block: the plain red of
/// dark themes has no ink that keeps its hue there.
fn failed(theme: &Theme, fill: u32) -> u32 {
    crate::contrast::ink_on_chrome(theme.palette[9], [fill; 3], 3.)
}

/// `body` tinted with `fill`, behind a bar in `bar`.
fn block(look: &Look<'_>, bar: u32, fill: u32, body: Div) -> Div {
    div()
        .flex()
        .min_w_0()
        .child(div().flex_none().w(px(BAR)).bg(rgb(bar)))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .child(look.ground(body.px(px(INSET - BAR)).py(px(8.)).min_w_0(), fill)),
        )
}

impl ChatLayout for OpenCode {
    fn ground(&self, side: Side, look: &Look<'_>) -> Option<u32> {
        Some(match side {
            Side::User => tint(look.theme, PROMPT_TINT),
            Side::Agent => look.theme.background,
        })
    }

    fn compact_settings(&self) -> bool {
        true
    }

    fn mono_prose(&self) -> bool {
        true
    }

    fn row(&self, row: Row<'_>, look: &Look<'_>) -> AnyElement {
        let Some(side) = row.side() else {
            return summary_row(row, look, 6.);
        };
        let theme = look.theme;
        let id = row.entry.id.0;
        let tool = row.tool;
        let open = row.expanded || tool.is_some_and(|tool| self.tool_fold(tool) != ToolFold::Line);
        let body = match (side, tool) {
            (Side::User, _) => block(
                look,
                theme.primary(),
                tint(theme, PROMPT_TINT),
                div().child(row.content),
            ),
            (_, Some(state)) if open => {
                let fill = tint(
                    theme,
                    if state == ToolState::Running {
                        RUNNING_TINT
                    } else {
                        TOOL_TINT
                    },
                );
                let bar = match state {
                    ToolState::Failed => failed(theme, fill),
                    ToolState::Running => Hues::of(theme).yellow,
                    _ => tint(theme, BAR_TINT),
                };
                block(look, bar, fill, div().child(row.content))
            }
            _ => div().pl(px(INSET)).pr(px(8.)).child(row.content),
        };
        // Folded calls in a row read as a list; everything else breathes.
        let above = if tool.is_some() && !open { 4. } else { 12. };
        look.column(
            div()
                .id(("chat-entry", id))
                .debug_selector(move || format!("chat-opencode-{id}"))
                .px(px(10.))
                .pt(px(above))
                .min_w_0()
                .text_font(look.mono)
                .text_size(px(look.mono.size))
                .child(body),
        )
        .into_any_element()
    }

    /// What needs attention stays in view; what finished is one line.
    fn tool_fold(&self, state: ToolState) -> ToolFold {
        match state {
            ToolState::Failed | ToolState::Running => ToolFold::Preview,
            ToolState::Done | ToolState::Pending | ToolState::Stopped => ToolFold::Line,
        }
    }

    fn jump_label(&self) -> bool {
        true
    }

    fn composer(&self, parts: ComposerParts, look: &Look<'_>) -> Div {
        let theme = look.theme;
        let small = px((look.mono.size - 1.).max(10.));
        let settings = div()
            .debug_selector(|| "chat-settings".into())
            .flex()
            .items_center()
            .gap(px(6.))
            .text_size(small)
            // The model yields width first. Exceptionally narrow panes can
            // scroll settings without displacing Stop.
            .child(
                div()
                    .id("chat-settings-options")
                    .debug_selector(|| "chat-settings-options".into())
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .overflow_x_scroll()
                    .items_center()
                    .gap(px(6.))
                    .child(parts.attach)
                    .children(parts.settings),
            )
            .child(parts.send);
        let status = div()
            .debug_selector(|| "chat-opencode-status".into())
            .flex()
            .items_center()
            .justify_between()
            .gap(px(12.))
            .px(px(2.))
            .text_size(small)
            .text_color(rgb(theme.muted))
            .child(div().flex_1().min_w_0().truncate().children(parts.place))
            .child(
                div()
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap(px(14.))
                    .children(parts.usage)
                    .children(parts.commands),
            );
        div()
            .flex()
            .flex_col()
            .gap(px(6.))
            .text_font(look.mono)
            .children(parts.chips)
            .child(block(
                look,
                theme.primary(),
                tint(theme, COMPOSER_TINT),
                div()
                    .flex()
                    .flex_col()
                    .gap(px(8.))
                    .child(parts.draft)
                    .child(settings),
            ))
            .child(status)
    }
}
