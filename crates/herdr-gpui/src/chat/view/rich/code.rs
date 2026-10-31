use super::*;
use crate::chat::{
    rich::{
        CodeText, MAX_CODE_LINE_CHARS, MAX_CODE_LINES,
        diff::{self, Kind as DiffKind},
    },
    selection::{Cell, Segment},
};
use crate::config::Theme;
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
        self.render_rich_code_block(
            CodeView {
                id,
                language,
                source,
                block,
                limit: MAX_CODE_LINES,
            },
            context.visuals,
            Some(context.selection),
            context.cx,
        )
    }

    pub(in crate::chat::view) fn render_chat_tool_code(
        &self,
        code: &crate::chat::prepared::tool::Code,
        selection: Option<&Scope<'_>>,
        expanded: bool,
        visuals: &Visuals,
        cx: &mut Context<Self>,
    ) -> Div {
        self.render_rich_code_block(
            CodeView {
                id: code.id,
                language: &code.language,
                source: &code.source,
                block: 0,
                limit: crate::chat::selection::tool_code_limit(&code.language, expanded),
            },
            visuals,
            selection,
            cx,
        )
    }

    fn render_rich_code_block(
        &self,
        code: CodeView<'_>,
        visuals: &Visuals,
        selection: Option<&Scope<'_>>,
        cx: &mut Context<Self>,
    ) -> Div {
        let CodeView {
            id,
            language,
            source,
            block,
            limit,
        } = code;
        let is_diff =
            language.eq_ignore_ascii_case("diff") || language.eq_ignore_ascii_case("patch");
        let diff_rows = is_diff.then(|| diff::rows(source, limit));
        let spans = visuals.code(id);
        // One pass: the lines past the limit are only checked for, not counted.
        let mut raw = source.lines();
        let lines: Vec<_> = raw
            .by_ref()
            .take(limit)
            .enumerate()
            .map(|(index, raw)| {
                let spans = spans
                    .and_then(|spans| spans.get(index))
                    .map_or(&[][..], Vec::as_slice);
                match selection
                    .and_then(|selection| {
                        selection.segment(Cell {
                            block,
                            row: index,
                            column: 0,
                        })
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
                        if is_diff {
                            "Diff"
                        } else if language.is_empty() {
                            "Code"
                        } else {
                            language
                        }
                        .to_owned(),
                    )
                    .child(self.rich_copy(
                        if is_diff { copy::DIFF } else { copy::CODE },
                        Arc::clone(source),
                        cx,
                    )),
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
                        let kind = diff_rows
                            .as_ref()
                            .and_then(|rows| rows.get(index))
                            .map(|row| row.kind);
                        let (background, ink) = diff_colors(&self.theme, kind);
                        let marks = diff_sign(&self.theme, kind, background)
                            .map(|color| {
                                (
                                    0..1,
                                    HighlightStyle {
                                        color: Some(rgb(color).into()),
                                        ..Default::default()
                                    },
                                )
                            })
                            .into_iter()
                            .chain(line.spans.into_iter().map(|span| {
                                (
                                    span.start..span.end,
                                    HighlightStyle {
                                        color: Some(
                                            rgb(span
                                                .token
                                                .colour(&self.theme, Contrast::On(background)))
                                            .into(),
                                        ),
                                        ..Default::default()
                                    },
                                )
                            }));
                        let fragment = selection.and_then(|selection| {
                            selection.fragment(Cell {
                                block,
                                row: index,
                                column: 0,
                            })
                        });
                        let styled = styled_text(fragment.as_ref(), line.text, marks, &self.theme);
                        let layout = styled.layout().clone();
                        let view = div()
                            .debug_selector(move || format!("chat-code-line-{index}"))
                            .flex_none()
                            .min_w_full()
                            .whitespace_nowrap()
                            .min_h(px(self.config.terminal.size * 1.5))
                            .bg(rgb(background))
                            .text_color(rgb(ink))
                            .when(is_diff, |row| row.px(px(6.)))
                            .child(styled);
                        selectable(fragment.as_ref(), view, layout, cx)
                    })),
            )
            .when(truncated, |view| {
                view.child(
                    div()
                        .debug_selector(|| "chat-code-truncated".into())
                        .px(px(14.))
                        .pb(px(6.))
                        .text_color(rgb(self.theme.muted))
                        .child(if limit < MAX_CODE_LINES {
                            "Expand for more lines; Copy includes the full source.".to_owned()
                        } else {
                            truncation_note()
                        }),
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

fn diff_colors(theme: &Theme, kind: Option<DiffKind>) -> (u32, u32) {
    let background = match kind {
        Some(DiffKind::Added) => crate::config::mix(theme.surface, theme.palette[2], 17),
        Some(DiffKind::Removed) => crate::config::mix(theme.surface, theme.palette[1], 17),
        _ => theme.surface,
    };
    let ink = if kind == Some(DiffKind::Meta) {
        theme.muted
    } else {
        theme.foreground
    };
    (background, super::super::text_ink(ink, background))
}

fn diff_sign(theme: &Theme, kind: Option<DiffKind>, background: u32) -> Option<u32> {
    let palette = match kind? {
        DiffKind::Added => 2,
        DiffKind::Removed => 1,
        _ => return None,
    };
    Some(super::super::text_ink(theme.palette[palette], background))
}

struct CodeView<'a> {
    id: VisualId,
    language: &'a str,
    source: &'a Arc<str>,
    block: usize,
    limit: usize,
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
