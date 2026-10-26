use super::*;
use crate::chat::{
    rich::{CodeText, MAX_CODE_LINE_CHARS, MAX_CODE_LINES},
    selection::{Cell, Segment},
};
use crate::review::highlight::{Contrast, Span};
use std::sync::Arc;

impl HerdrWindow {
    pub(super) fn rich_code(
        &self,
        id: VisualId,
        language: &str,
        source: &Arc<str>,
        block: usize,
        context: &mut RichContext<'_, '_, '_>,
    ) -> Div {
        let spans = context.visuals.code(id);
        // One pass: the lines past the limit are only checked for, not counted.
        let mut raw = source.lines();
        let lines: Vec<_> = raw
            .by_ref()
            .take(MAX_CODE_LINES)
            .enumerate()
            .map(|(index, raw)| {
                let spans = spans
                    .and_then(|spans| spans.get(index))
                    .map_or(&[][..], Vec::as_slice);
                match context
                    .selection
                    .segment(Cell {
                        block,
                        row: index,
                        column: 0,
                    })
                    .and_then(Segment::code)
                {
                    Some(code) => highlighted(code, raw, spans),
                    None => code_line(raw, spans),
                }
            })
            .collect();
        let truncated = raw.next().is_some() || lines.iter().any(|line| line.truncated);
        div()
            .debug_selector(|| "chat-code-block".into())
            .min_w_0()
            .rounded(px(6.))
            .border_1()
            .border_color(rgb(self.theme.active))
            .bg(rgb(self.theme.surface))
            .text_color(rgb(self.theme.foreground))
            .overflow_hidden()
            .child(
                div()
                    .flex()
                    .justify_between()
                    .items_center()
                    .px(px(14.))
                    .py(px(6.))
                    .border_b_1()
                    .border_color(rgb(self.theme.active))
                    .text_size(px(self.chat_text_size() - 2.))
                    .text_color(rgb(self.theme.muted))
                    .child(
                        if language.is_empty() {
                            "Code"
                        } else {
                            language
                        }
                        .to_owned(),
                    )
                    .child(self.rich_copy(copy::CODE, Arc::clone(source), context.cx)),
            )
            .child(
                div()
                    .id("code-scroll")
                    .debug_selector(|| "chat-code-scroll".into())
                    .min_w_0()
                    .overflow_x_scroll()
                    .flex()
                    .flex_col()
                    .items_start()
                    .p(px(14.))
                    .text_size(px(self.config.terminal.size))
                    .line_height(relative(1.5))
                    .text_font(&self.config.terminal)
                    .children(lines.into_iter().enumerate().map(|(index, line)| {
                        let marks = line.spans.into_iter().map(|span| {
                            (
                                span.start..span.end,
                                HighlightStyle {
                                    color: Some(
                                        rgb(span
                                            .token
                                            .colour(&self.theme, Contrast::On(self.theme.surface)))
                                        .into(),
                                    ),
                                    ..Default::default()
                                },
                            )
                        });
                        let fragment = context.selection.fragment(Cell {
                            block,
                            row: index,
                            column: 0,
                        });
                        let styled = styled_text(fragment.as_ref(), line.text, marks, &self.theme);
                        let layout = styled.layout().clone();
                        let view = div()
                            .debug_selector(move || format!("chat-code-line-{index}"))
                            .flex_none()
                            .whitespace_nowrap()
                            .min_h(px(self.config.terminal.size * 1.5))
                            .child(styled);
                        selectable(fragment.as_ref(), view, layout, context.cx)
                    })),
            )
            .when(truncated, |view| {
                view.child(
                    div()
                        .debug_selector(|| "chat-code-truncated".into())
                        .px(px(14.))
                        .pb(px(6.))
                        .text_color(rgb(self.theme.muted))
                        .child(truncation_note()),
                )
            })
    }
}

/// What a cut code block says under its last line, from the limits that
/// cut it.
fn truncation_note() -> String {
    format!(
        "Preview limited to {} lines and {} characters per line; Copy code includes the full block.",
        grouped(MAX_CODE_LINES),
        grouped(MAX_CODE_LINE_CHARS),
    )
}

/// `number` with commas between its groups of thousands.
fn grouped(number: usize) -> String {
    let digits = number.to_string();
    let mut text = String::with_capacity(digits.len() + digits.len() / 3);
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            text.push(',');
        }
        text.push(digit);
    }
    text
}

struct CodeLine {
    text: String,
    spans: Vec<Span>,
    truncated: bool,
}

/// A line without a selectable segment, as when selection is unavailable.
fn code_line(raw: &str, spans: &[Span]) -> CodeLine {
    highlighted(&CodeText::expand(raw), raw, spans)
}

/// Tabs are layout, not unsafe controls. Grammar byte offsets map through
/// the drawn line's expansion; the original source remains the copy payload.
/// A prepared segment's line is reused, so drawing and selecting expanded
/// tabs cannot drift apart and the map is not rebuilt per frame.
fn highlighted(code: &CodeText, raw: &str, spans: &[Span]) -> CodeLine {
    let spans = spans
        .iter()
        .filter_map(|span| {
            let start = code.shown(span.start)?;
            let end = code.shown(span.end.min(code.end))?;
            (start < end).then_some(Span {
                start,
                end,
                token: span.token,
            })
        })
        .collect();
    CodeLine {
        text: code.text.clone(),
        spans,
        truncated: code.end < raw.len(),
    }
}

#[cfg(test)]
mod tests;
