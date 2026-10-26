//! Native rich blocks. No HTML is executed and code-copy buttons only copy.
use crate::chat::{
    rich::{Block, Inline, Kind, Style},
    visual::{Shown, VisualId, Visuals},
};
use crate::{HerdrWindow, config::ChatLayoutMode, fonts::StyledFont};
use gpui::{prelude::*, *};
use pulldown_cmark::{Alignment, HeadingLevel};
use std::ops::Range;

mod code;
mod links;

use super::{
    copy,
    layout::layout_for,
    selection::{Fragment, Scope, selectable, styled_text},
};
use crate::chat::selection::Cell;
use links::Links;

struct RichContext<'a, 'b, 'c> {
    visuals: &'a Visuals,
    links: Links,
    selection: &'a Scope<'a>,
    cx: &'b mut Context<'c, HerdrWindow>,
}

impl HerdrWindow {
    pub(super) fn render_chat_rich(
        &self,
        pane_id: &str,
        blocks: &[Block],
        visuals: &Visuals,
        selection: &Scope<'_>,
        cx: &mut Context<Self>,
    ) -> Div {
        let links = Links::new(self, pane_id, cx);
        let look = self.chat_look();
        let layout = layout_for(self.config.chat.layout);
        let mut context = RichContext {
            visuals,
            links,
            selection,
            cx,
        };
        div()
            .flex()
            .flex_col()
            .gap(px(12.))
            .min_w_0()
            .when(
                self.config.chat.layout != ChatLayoutMode::Terminal,
                |view| view.text_font(&self.config.ui),
            )
            .text_size(px(self.chat_text_size()))
            .when_some(layout.prose(&look), |view, ink| view.text_color(rgb(ink)))
            .line_height(relative(1.6))
            .children(blocks.iter().enumerate().map(|(index, block)| {
                let element = match block {
                    Block::Paragraph {
                        kind,
                        indent,
                        content,
                    } => self
                        .rich_paragraph(*kind, *indent, content, index, &mut context)
                        .into_any_element(),
                    Block::Math { id, source } => {
                        let formula = self.rich_math(*id, source, visuals, false);
                        let formula = match selection.fragment(Cell::block(index)) {
                            Some(fragment) => fragment.atomic(formula, &self.theme, context.cx),
                            None => formula,
                        };
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(4.))
                            .child(formula)
                            .child(self.rich_copy(copy::LATEX, source.clone(), context.cx))
                            .into_any_element()
                    }
                    Block::Code {
                        id,
                        language,
                        source,
                    } => self
                        .rich_code(*id, language, source, index, &mut context)
                        .into_any_element(),
                    Block::Table { align, rows } => self
                        .rich_table(align, rows, index, &mut context)
                        .into_any_element(),
                };
                div().id(("rich-block", index)).min_w_0().child(element)
            }))
    }

    fn rich_paragraph(
        &self,
        kind: Kind,
        indent: usize,
        content: &[Inline],
        block: usize,
        context: &mut RichContext<'_, '_, '_>,
    ) -> Div {
        let row = div().min_w_0().pl(px(indent.min(6) as f32 * 20.));
        if kind == Kind::Rule {
            return row.h(px(1.)).bg(rgb(self.theme.active));
        }
        let row = row
            .when_some(heading(kind), |row, level| {
                row.font_weight(FontWeight::SEMIBOLD)
                    .text_size(px(self.chat_text_size() * heading_scale(level)))
                    .line_height(relative(1.3))
                    .pt(px(10.))
                    .pb(px(2.))
            })
            .when(kind == Kind::Quote, |row| {
                row.pl(px(10.))
                    .border_l_2()
                    .border_color(rgb(self.theme.active))
            });
        let marker = match kind {
            Kind::Bullet => Some("•".to_owned()),
            Kind::Numbered(n) => Some(format!("{n}.")),
            _ => None,
        };
        if let Some(marker) = marker {
            row.flex()
                .gap(px(8.))
                .child(div().flex_none().child(marker))
                .child(
                    self.rich_inline(
                        content,
                        context.selection.fragment(Cell::block(block)),
                        context,
                    )
                    .flex_1(),
                )
        } else {
            row.child(self.rich_inline(
                content,
                context.selection.fragment(Cell::block(block)),
                context,
            ))
        }
    }

    /// Text words and formulas share one wrapping flow; an inline formula
    /// never becomes its own block. Words retain their whitespace and style.
    fn rich_inline(
        &self,
        content: &[Inline],
        fragment: Option<Fragment>,
        context: &mut RichContext<'_, '_, '_>,
    ) -> Div {
        let content = context.links.content(content);
        // Shape prose as a single run sequence: preserves kerning, wrapping,
        // links and inline-code fonts without allocating one element per word.
        if let Some(Prose { text, spans }) = prose_spans(&content) {
            let highlights = spans
                .iter()
                .map(|(range, style)| (range.clone(), self.rich_style(style)));
            let fonts = spans
                .iter()
                .filter(|(_, style)| style.code)
                .map(|(range, _)| (range.clone(), self.config.terminal.family.clone().into()));
            let text = styled_text(fragment.as_ref(), text, highlights, &self.theme)
                .with_font_family_overrides(fonts);
            let layout = text.layout().clone();
            let targets: Vec<_> = spans
                .iter()
                .filter_map(|(range, style)| {
                    style.link.as_ref().map(|url| (range.clone(), url.clone()))
                })
                .collect();
            // A flex row gives bare text its intrinsic width; a block gives
            // shaping the actual column width so long paragraphs wrap.
            let view = div()
                .debug_selector(|| "chat-prose".into())
                .w_full()
                .min_w_0();
            let content = if targets.is_empty() {
                text.into_any_element()
            } else {
                let ranges = targets.iter().map(|(range, _)| range.clone()).collect();
                let links = context.links.clone();
                let guard = fragment.clone();
                InteractiveText::new("rich-prose", text)
                    .on_click(ranges, move |index, _, cx| {
                        if guard.as_ref().is_none_or(Fragment::allows_link)
                            && let Some((_, url)) = targets.get(index)
                        {
                            links.open(url, cx);
                        }
                    })
                    .into_any_element()
            };
            // The selection canvas precedes glyphs so aligned background
            // marks paint behind the text and retain normal text contrast.
            return selectable(fragment.as_ref(), view, layout, context.cx).child(content);
        }
        let mut children = Vec::new();
        let mut offset = 0;
        for part in content.iter() {
            match part {
                Inline::Text(text, style) => {
                    for word in text.split_inclusive(char::is_whitespace) {
                        let index = children.len();
                        let mut item = div().flex_none().max_w_full().when(style.code, |item| {
                            item.text_font(&self.config.terminal)
                                .bg(rgb(self.theme.active))
                                .rounded(px(2.))
                        });
                        let selected = fragment
                            .as_ref()
                            .map(|fragment| fragment.range(offset..offset + word.len()));
                        offset += word.len();
                        let marks = [(0..word.len(), self.rich_style(style))];
                        let styled =
                            styled_text(selected.as_ref(), word.to_owned(), marks, &self.theme);
                        item = selectable(
                            selected.as_ref(),
                            item,
                            styled.layout().clone(),
                            context.cx,
                        );
                        item = if let Some(url) = &style.link {
                            let url = url.clone();
                            let links = context.links.clone();
                            let guard = selected.clone();
                            item.child(InteractiveText::new(("rich-link", index), styled).on_click(
                                std::iter::once(0..word.len()).collect(),
                                move |_, _, cx| {
                                    if guard.as_ref().is_none_or(Fragment::allows_link) {
                                        links.open(&url, cx);
                                    }
                                },
                            ))
                        } else {
                            item.child(styled)
                        };
                        children.push(item.into_any_element());
                    }
                }
                Inline::Math { id, source } => {
                    let formula = self.rich_math(*id, source, context.visuals, true);
                    let formula = match &fragment {
                        Some(fragment) => fragment.range(offset..offset + source.len()).atomic(
                            formula,
                            &self.theme,
                            context.cx,
                        ),
                        None => formula,
                    };
                    offset += source.len();
                    children.push(formula.into_any_element());
                }
                Inline::Break => {
                    offset += 1;
                    children.push(div().w_full().h(px(0.)).into_any_element());
                }
            }
        }
        div()
            .flex()
            .flex_wrap()
            .items_baseline()
            .min_w_0()
            .gap_y(px(2.))
            .children(children)
    }

    fn rich_math(&self, id: VisualId, source: &str, visuals: &Visuals, inline: bool) -> Div {
        // Math textures use the theme's ink. Keep that surface behind them
        // when a layout supplies a different message fill, without re-rasterizing.
        let backdrop = layout_for(self.config.chat.layout).backdrop(&self.chat_look());
        let ground =
            |view: Div| view.when_some(backdrop, |view, fill| view.bg(rgb(fill)).rounded(px(2.)));
        let shown = visuals.shown(id);
        let Shown::Ready(drawn) = shown else {
            let source =
                crate::notifications::safe_text(source, super::super::rich::MAX_MATH_BYTES);
            let fallback = div()
                .max_w_full()
                .text_font(&self.config.terminal)
                .text_color(rgb(self.theme.subtext()))
                .child(if inline {
                    format!("${source}$")
                } else {
                    format!("$${source}$$")
                });
            return ground(if !inline && matches!(shown, Shown::Failed(_)) {
                div()
                    .flex()
                    .flex_col()
                    .gap(px(4.))
                    .min_w_0()
                    .child(
                        div()
                            .text_size(px((self.chat_text_size() - 2.).max(10.)))
                            .text_color(rgb(self.theme.subtext()))
                            .child("Equation preview unavailable; original LaTeX is shown below."),
                    )
                    .child(fallback)
            } else {
                fallback
            });
        };
        // The zero-width text establishes a text baseline inside the math
        // atom; the picture is positioned against its measured TeX baseline.
        ground(if inline {
            let text_baseline = self.chat_text_size() * 0.8;
            let baseline = drawn.baseline.unwrap_or(drawn.height);
            let top = (text_baseline - baseline).max(0.);
            let before = (baseline - text_baseline).max(0.);
            div()
                .debug_selector(|| "chat-math-inline".into())
                .flex_none()
                .relative()
                .max_w_full()
                .w(px(drawn.width))
                .h(px(
                    (drawn.height + top).max(self.chat_text_size() * 1.4 + before)
                ))
                .child(div().pt(px(before)).child("\u{200b}"))
                .child(
                    img(drawn.texture.clone())
                        .absolute()
                        .top(px(top))
                        .left(px(0.))
                        .w(px(drawn.width))
                        .h(px(drawn.height))
                        .max_w_full()
                        .object_fit(ObjectFit::Contain),
                )
        } else {
            div()
                .debug_selector(|| "chat-math-display".into())
                .min_w_0()
                .max_w_full()
                .py(px(8.))
                .child(
                    img(drawn.texture.clone())
                        .w(px(drawn.width))
                        .h(px(drawn.height))
                        .max_w_full()
                        .object_fit(ObjectFit::Contain),
                )
        })
    }

    fn rich_table(
        &self,
        align: &[Alignment],
        rows: &[Vec<Vec<Inline>>],
        block: usize,
        context: &mut RichContext<'_, '_, '_>,
    ) -> Stateful<Div> {
        let columns = rows.iter().map(Vec::len).max().unwrap_or(0);
        let widths: Vec<_> = (0..columns)
            .map(|column| {
                let length = rows
                    .iter()
                    .filter_map(|row| row.get(column))
                    .map(|cell| {
                        cell.iter()
                            .map(|inline| match inline {
                                Inline::Text(text, _) => text.chars().count(),
                                Inline::Math { source, .. } => source.chars().count(),
                                Inline::Break => 0,
                            })
                            .sum::<usize>()
                    })
                    .max()
                    .unwrap_or(0);
                (length as f32 * self.chat_text_size() * 0.5 + 24.).clamp(100., 260.)
            })
            .collect();
        let width: f32 = widths.iter().sum();
        div()
            .id("table-scroll")
            .debug_selector(|| "chat-rich-table".into())
            .min_w_0()
            .overflow_x_scroll()
            .rounded(px(5.))
            .border_1()
            .border_color(rgb(self.theme.active))
            .bg(rgb(self.theme.background))
            .text_color(rgb(self.theme.foreground))
            .children(rows.iter().enumerate().map(|(row_index, row)| {
                div()
                    .flex()
                    .w_full()
                    .min_w(px(width))
                    .when(row_index == 0, |row| {
                        row.bg(rgb(self.theme.surface))
                            .font_weight(FontWeight::SEMIBOLD)
                    })
                    .when(row_index > 0, |row| {
                        row.border_t_1().border_color(rgb(self.theme.active))
                    })
                    .children(row.iter().enumerate().map(|(column, content)| {
                        let selection = context
                            .selection
                            .fragment(Cell {
                                block,
                                row: row_index,
                                column,
                            })
                            .map(|fragment| match align.get(column) {
                                Some(Alignment::Center | Alignment::Right) => fragment.aligned(),
                                _ => fragment,
                            });
                        div()
                            .id(("cell", row_index * columns + column))
                            .debug_selector(move || format!("chat-table-cell-{row_index}-{column}"))
                            .flex_grow(1.)
                            .flex_shrink_0()
                            .flex_basis(px(widths[column]))
                            .min_w(px(widths[column]))
                            .px(px(10.))
                            .py(px(7.))
                            .when(column > 0, |cell| {
                                cell.border_l_1().border_color(rgb(self.theme.active))
                            })
                            .child(
                                self.rich_inline(content, selection, context)
                                    .when(align.get(column) == Some(&Alignment::Center), |cell| {
                                        cell.justify_center().text_align(TextAlign::Center)
                                    })
                                    .when(align.get(column) == Some(&Alignment::Right), |cell| {
                                        cell.justify_end().text_align(TextAlign::Right)
                                    }),
                            )
                    }))
            }))
    }

    fn rich_style(&self, style: &Style) -> HighlightStyle {
        HighlightStyle {
            font_weight: style.bold.then_some(FontWeight::SEMIBOLD),
            font_style: style.italic.then_some(FontStyle::Italic),
            color: style
                .link
                .as_ref()
                .and_then(|_| layout_for(self.config.chat.layout).link(&self.chat_look()))
                .or_else(|| style.code.then_some(self.theme.foreground))
                .map(|color| rgb(color).into()),
            underline: style.link.as_ref().map(|_| UnderlineStyle {
                thickness: px(1.),
                ..Default::default()
            }),
            strikethrough: style.strike.then_some(StrikethroughStyle {
                thickness: px(1.),
                ..Default::default()
            }),
            background_color: style.code.then(|| rgb(self.theme.active).into()),
            ..Default::default()
        }
    }
}

fn heading(kind: Kind) -> Option<HeadingLevel> {
    match kind {
        Kind::Heading(level) => Some(level),
        _ => None,
    }
}

fn heading_scale(level: HeadingLevel) -> f32 {
    match level {
        HeadingLevel::H1 => 1.65,
        HeadingLevel::H2 => 1.4,
        HeadingLevel::H3 => 1.2,
        _ => 1.05,
    }
}

/// Byte ranges are shared by style, font and link handling, including Unicode.
/// Mixed formulas use the inline-atom layout; normal text stays one element.
struct Prose<'a> {
    text: String,
    spans: Vec<(Range<usize>, &'a Style)>,
}

fn prose_spans(content: &[Inline]) -> Option<Prose<'_>> {
    if content
        .iter()
        .any(|part| matches!(part, Inline::Math { .. }))
    {
        return None;
    }
    let mut text = String::new();
    let mut spans = Vec::new();
    for part in content {
        match part {
            Inline::Text(value, style) => {
                let start = text.len();
                text.push_str(value);
                if start != text.len() {
                    spans.push((start..text.len(), style));
                }
            }
            Inline::Break => text.push('\n'),
            Inline::Math { .. } => return None,
        }
    }
    Some(Prose { text, spans })
}

#[cfg(test)]
mod tests;
