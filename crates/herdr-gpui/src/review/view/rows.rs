//! Drawing the diff's lines, unified or side by side, and dragging the line
//! between the sides. Every line cell starts a note on its own row, so a
//! side-by-side change is noted on the side the user clicked. A row asks for
//! what it lacks as it is drawn, its file's lines or its colours, and draws
//! what it has meanwhile.
use super::{Review, model::Item};
use crate::browser::TabId;
use crate::{
    HerdrWindow,
    config::Theme,
    review::{
        diff::{Body, Kind, Lines, RowId, SplitRow},
        highlight::{Span, Token},
    },
};
use gpui::{prelude::*, *};
use std::ops::Range;

/// The old side's share of a side-by-side row until it is dragged, and how
/// narrow either side may get.
pub(super) const EVEN_SPLIT: f32 = 0.5;
const MIN_SIDE: f32 = 0.2;

/// How a line of `kind` is marked and coloured: its sign, its tint, its
/// changed words' stronger tint, and its text colour.
fn look(theme: &Theme, kind: Kind) -> (&'static str, Option<Rgba>, Option<Rgba>, u32) {
    let tint = |color: u32, alpha: u32| rgba((color << 8) | alpha);
    match kind {
        Kind::Added => (
            "+",
            Some(tint(theme.palette[2], 0x2c)),
            Some(tint(theme.palette[2], 0x70)),
            theme.foreground,
        ),
        Kind::Removed => (
            "-",
            Some(tint(theme.palette[1], 0x2c)),
            Some(tint(theme.palette[1], 0x70)),
            theme.foreground,
        ),
        Kind::Context => (" ", None, None, theme.foreground),
        Kind::Hunk | Kind::Meta => ("", None, None, theme.muted),
    }
}

/// A token's colour in this theme: the terminal palette's, made readable
/// on the panel and its add and remove tints.
fn token_colour(theme: &Theme, token: Token) -> Rgba {
    rgb(match token {
        Token::Comment => theme.muted,
        Token::String => theme.ink(theme.palette[2]),
        Token::Number | Token::Constant => theme.ink(theme.palette[3]),
        Token::Keyword => theme.ink(theme.palette[5]),
        Token::Type => theme.ink(theme.palette[6]),
        Token::Function => theme.ink(theme.palette[4]),
    })
}

/// A line's code, coloured by its syntax where known, its changed words on
/// a stronger tint. The two may overlap, so the text is cut at every edge
/// of either.
fn code(
    theme: &Theme,
    text: &str,
    spans: &[Span],
    words: &[Range<u32>],
    word_tint: Option<Rgba>,
) -> AnyElement {
    let words: Vec<Range<usize>> = word_tint
        .map(|_| {
            words
                .iter()
                .map(|word| word.start as usize..(word.end as usize).min(text.len()))
                .filter(|word| word.start < word.end)
                .collect()
        })
        .unwrap_or_default();
    if spans.is_empty() && words.is_empty() {
        return SharedString::from(text.to_owned()).into_any_element();
    }
    let mut edges: Vec<usize> = spans
        .iter()
        .flat_map(|span| [span.start, span.end])
        .chain(words.iter().flat_map(|word| [word.start, word.end]))
        .filter(|&edge| edge <= text.len() && text.is_char_boundary(edge))
        .collect();
    edges.sort_unstable();
    edges.dedup();
    let mut highlights = Vec::new();
    for pair in edges.windows(2) {
        let range = pair[0]..pair[1];
        let colour = spans
            .iter()
            .find(|span| span.start <= range.start && range.end <= span.end)
            .map(|span| token_colour(theme, span.token).into());
        let background = words
            .iter()
            .any(|word| word.start <= range.start && range.end <= word.end)
            .then_some(word_tint)
            .flatten()
            .map(Into::into);
        if colour.is_some() || background.is_some() {
            highlights.push((
                range,
                HighlightStyle {
                    color: colour,
                    background_color: background,
                    ..HighlightStyle::default()
                },
            ));
        }
    }
    StyledText::new(text.to_owned())
        .with_highlights(highlights)
        .into_any_element()
}

fn number(theme: &Theme, value: Option<u32>) -> Div {
    div()
        .flex_none()
        .w(px(44.))
        .pr_1()
        .flex()
        .justify_end()
        .text_color(rgb(theme.muted))
        .child(value.map(|value| value.to_string()).unwrap_or_default())
}

/// The slot a note's number shows in, empty without one.
pub(super) fn mark_slot(theme: &Theme, mark: Option<usize>) -> Div {
    div()
        .flex_none()
        .w(px(20.))
        .flex()
        .items_center()
        .justify_center()
        .when_some(mark, |slot, mark| {
            slot.child(
                div()
                    .size(px(16.))
                    .rounded_full()
                    .bg(rgb(theme.palette[3]))
                    .text_color(rgb(theme.text_on(theme.palette[3])))
                    .text_size(px(10.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(mark.to_string()),
            )
        })
}

/// Which line number a cell shows: the old one on the left of a
/// side-by-side row, otherwise the new one where there is one.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Numbers {
    Both,
    Old,
    New,
}

impl Numbers {
    fn selector(self) -> &'static str {
        match self {
            Self::Both => "line",
            Self::Old => "left",
            Self::New => "right",
        }
    }
}

/// The lines of `file` in `review`, if read.
fn lines_of(review: &Review, file: usize) -> Option<&std::sync::Arc<Lines>> {
    review.loaded()?.diff.files.get(file)?.lines()
}

impl HerdrWindow {
    /// The list's row at `position`, in the review's layout.
    pub(super) fn review_row(
        &mut self,
        id: TabId,
        position: usize,
        line_height: f32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.prepare_review_row(id, position, cx);
        self.render_review_item(id, position, line_height, cx)
            .unwrap_or_else(|| div().h(px(line_height)).into_any_element())
    }

    /// Asks for what the row at `position` needs before it is drawn: its
    /// file's lines, or its colours.
    fn prepare_review_row(&mut self, id: TabId, position: usize, cx: &mut Context<Self>) {
        let Some(review) = self.reviews.get(&id) else {
            return;
        };
        let Some(item) = review.item(position) else {
            return;
        };
        // A side-by-side pair may join two stretches: both are asked for.
        let (file, lines) = match item {
            Item::Placeholder(file) => {
                let pending = review
                    .loaded()
                    .and_then(|loaded| loaded.diff.files.get(file))
                    .is_some_and(|entry| entry.body == Body::Pending && !entry.folded);
                if pending {
                    self.want_review_body(id, file, cx);
                }
                return;
            }
            Item::Header(_) => return,
            Item::Line { file, line } => (file, [Some(line), None]),
            Item::Pair { file, row } => {
                match lines_of(review, file).and_then(|lines| lines.split().get(row).copied()) {
                    Some(SplitRow::Sides { left, right }) => (file, [left, right]),
                    _ => return,
                }
            }
        };
        let Some(read) = lines_of(review, file) else {
            return;
        };
        let needed: Vec<usize> = lines
            .into_iter()
            .flatten()
            .filter(|&line| {
                read.get(line).is_some_and(|found| {
                    matches!(found.kind, Kind::Added | Kind::Removed | Kind::Context)
                }) && review
                    .colours
                    .line(file, read.stretch(line).start, line)
                    .is_none()
            })
            .collect();
        for line in needed {
            self.want_review_colours(id, file, line, cx);
        }
    }

    fn render_review_item(
        &self,
        id: TabId,
        position: usize,
        line_height: f32,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let review = self.reviews.get(&id)?;
        let item = review.item(position)?;
        let mut line = |file: usize, line: usize, numbers: Numbers| {
            self.review_cell(id, review, file, line, numbers, line_height, cx)
        };
        Some(match item {
            Item::Header(file) => self.review_file_header(id, review, file, line_height, cx),
            Item::Placeholder(file) => self.review_placeholder(id, review, file, line_height, cx),
            // Tints span the list, not the text.
            Item::Line { file, line: index } => line(file, index, Numbers::Both)?
                .w_full()
                .into_any_element(),
            Item::Pair { file, row } => match *lines_of(review, file)?.split().get(row)? {
                SplitRow::Across(index) => line(file, index, Numbers::Both)?
                    .w_full()
                    .into_any_element(),
                SplitRow::Sides { left, right } => {
                    let theme = &self.theme;
                    let gap = rgba((theme.active << 8) | 0x60);
                    let mut half = |index: Option<usize>, numbers: Numbers| -> Stateful<Div> {
                        let cell = index.and_then(|index| line(file, index, numbers));
                        // Nothing on this side: a quiet gap, as tall as the other.
                        cell.unwrap_or_else(|| {
                            div()
                                .id(("review-gap", position))
                                .min_h(px(line_height))
                                .bg(gap)
                        })
                        .flex_1()
                        .min_w_0()
                    };
                    // The row is as tall as its taller side; both stretch to it.
                    div()
                        .id(("review-split", position))
                        .w_full()
                        .flex()
                        .child(
                            half(left, Numbers::Old)
                                .flex_none()
                                .w(relative(review.split_ratio)),
                        )
                        .child(
                            half(right, Numbers::New)
                                .border_l_1()
                                .border_color(rgb(theme.active)),
                        )
                        .into_any_element()
                }
            },
        })
    }

    /// Line `index` of `file`, drawn as a cell that notes it when it can
    /// take a note.
    #[allow(clippy::too_many_arguments)]
    fn review_cell(
        &self,
        id: TabId,
        review: &Review,
        file: usize,
        index: usize,
        numbers: Numbers,
        line_height: f32,
        cx: &mut Context<Self>,
    ) -> Option<Stateful<Div>> {
        let theme = &self.theme;
        let lines = lines_of(review, file)?;
        let found = lines.get(index)?;
        let row = RowId::Line { file, line: index };
        let side = numbers.selector();
        let (sign, background, word_tint, text) = look(theme, found.kind);
        let noteable = matches!(found.kind, Kind::Added | Kind::Removed | Kind::Context);
        // An unchanged line shows on both sides; its number shows once.
        let mark = review
            .marks
            .get(&row)
            .copied()
            .filter(|_| !(found.kind == Kind::Context && numbers == Numbers::Old));
        let found_here = review.search.found(row);
        // Long lines wrap, as on GitHub: every text in the row keeps the
        // diff's line height, so the gutter lines up with the first line.
        let cell = div()
            .id((SharedString::from(format!("review-{side}-{file}")), index))
            .debug_selector(move || format!("review-{side}-{file}-{index}"))
            .min_h(px(line_height))
            .line_height(px(line_height))
            .flex()
            .items_start()
            .when_some(background, |cell, background| cell.bg(background))
            .when_some(found_here, |cell, current| {
                let alpha = if current { 0x80 } else { 0x38 };
                cell.bg(rgba((theme.palette[3] << 8) | alpha))
            })
            .when(review.draft == Some(row), |cell| cell.bg(rgb(theme.active)))
            .text_color(rgb(text))
            .when(noteable, |cell| {
                cell.cursor_pointer()
                    .hover(|cell| cell.bg(rgb(theme.active)))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        cx.stop_propagation();
                        this.begin_review_note(id, row, window, cx);
                    }))
            })
            .child(mark_slot(theme, mark).h(px(line_height)));
        if found.kind == Kind::Hunk {
            return Some(
                cell.child(self.review_expander(id, review, file, index, cx))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .child(lines.text_of(found).to_owned()),
                    ),
            );
        }
        let cell = match numbers {
            Numbers::Both => cell
                .child(number(theme, found.old))
                .child(number(theme, found.new)),
            Numbers::Old => cell.child(number(theme, found.old)),
            Numbers::New => cell.child(number(theme, found.new)),
        };
        let (spans, words) = review
            .colours
            .line(file, lines.stretch(index).start, index)
            .unwrap_or_default();
        Some(cell.child(div().flex_none().w(px(16.)).child(sign)).child(
            div().flex_1().min_w_0().child(code(
                theme,
                lines.text_of(found),
                spans,
                words,
                word_tint,
            )),
        ))
    }

    /// Before a hunk header, the unchanged lines above it to show, if any.
    fn review_expander(
        &self,
        id: TabId,
        review: &Review,
        file: usize,
        header: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = &self.theme;
        let gap = lines_of(review, file).and_then(|lines| lines.gap(header));
        let Some(gap) = gap else {
            // Lines up with the numbers' column on rows that have them.
            return div().flex_none().w(px(104.)).into_any_element();
        };
        let reading = review.expanding.contains(&(file, header));
        let label = if reading {
            "Reading\u{2026}".to_owned()
        } else {
            let count = gap.len();
            format!(
                "\u{2195} {count} more line{}",
                if count == 1 { "" } else { "s" }
            )
        };
        div()
            .id(ElementId::named_usize(
                format!("review-expand-{file}"),
                header,
            ))
            .debug_selector(move || format!("review-expand-{file}-{header}"))
            .flex_none()
            .w(px(104.))
            .px_1()
            .text_color(rgb(theme.foreground))
            .cursor_pointer()
            .rounded(px(crate::config::corners::CONTROL))
            .hover(|button| button.bg(rgb(theme.active)))
            .child(label)
            .on_click(cx.listener(move |this, _, _, cx| {
                cx.stop_propagation();
                this.expand_review_hunk(id, file, header, cx);
            }))
            .into_any_element()
    }

    /// Moves the line between the sides to the pointer at `x`, within the
    /// list laid out at `bounds`; whether it moved.
    pub(super) fn drag_review_split(
        &mut self,
        id: TabId,
        bounds: Bounds<Pixels>,
        x: Pixels,
    ) -> bool {
        let Some(review) = self.reviews.get_mut(&id) else {
            return false;
        };
        let width = f32::from(bounds.size.width);
        if width <= 0. {
            return false;
        }
        let ratio = (f32::from(x - bounds.left()) / width).clamp(MIN_SIDE, 1. - MIN_SIDE);
        if (ratio - review.split_ratio).abs() < f32::EPSILON {
            return false;
        }
        review.split_ratio = ratio;
        true
    }

    /// Back to even sides.
    pub(super) fn reset_review_split(&mut self, id: TabId) {
        if let Some(review) = self.reviews.get_mut(&id) {
            review.split_ratio = EVEN_SPLIT;
        }
    }
}
