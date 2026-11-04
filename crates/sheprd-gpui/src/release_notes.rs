//! Daemon release notes and product announcements, prepared for display.
//!
//! Both arrive from the daemon as Markdown-flavored text, which is untrusted:
//! it is bounded, stripped of control and bidirectional override characters,
//! and only ever drawn as text. Links become clickable ranges that open in the
//! browser on click, and only `http`/`https` targets qualify. Commands shown in
//! code spans or blocks are never run.
// i18n: migrated
use crate::{
    config::{FontConfig, Theme},
    fonts::StyledFont,
    notifications::safe_text,
};
use gpui::{prelude::*, *};
use std::{cell::RefCell, ops::Range, rc::Rc};

/// Scanned bytes of a body; a longer body is cut at a character boundary.
const MAX_BODY: usize = 64 * 1024;
const MAX_LINES: usize = 400;
const MAX_LINE: usize = 2000;
pub(crate) const MAX_URL: usize = 1024;
/// List nesting levels drawn; deeper items line up with the last.
const MAX_INDENT: usize = 4;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    Heading,
    Bullet,
    /// An ordered list item, with the number it was written with.
    Numbered(u32),
    /// A `>` quoted line.
    Quote,
    /// A `---` thematic break.
    Rule,
    /// A `|`-separated table row, its cells joined by a bar.
    Table,
    Paragraph,
    /// A line inside a fenced code block, drawn verbatim in the terminal font.
    Code,
    Blank,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Link {
    pub range: Range<usize>,
    pub url: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Line {
    pub kind: Kind,
    pub text: String,
    /// Byte ranges of inline code spans in `text`.
    pub code: Vec<Range<usize>>,
    /// Byte ranges of `**strong**` spans in `text`.
    pub bold: Vec<Range<usize>>,
    /// Byte ranges of `*emphasis*` and `_emphasis_` spans in `text`.
    pub italic: Vec<Range<usize>>,
    pub links: Vec<Link>,
    /// How deep a list item is nested, from its leading spaces.
    pub indent: u8,
}

impl Line {
    fn plain(kind: Kind, text: String) -> Self {
        Self {
            kind,
            text,
            code: Vec::new(),
            bold: Vec::new(),
            italic: Vec::new(),
            links: Vec::new(),
            indent: 0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mark {
    Code,
    Bold,
    Italic,
    Link,
}

impl Line {
    /// Styled ranges in text order, as styled text runs require.
    fn highlights(&self) -> impl Iterator<Item = (Range<usize>, Mark)> {
        let mut ranges: Vec<_> = self
            .code
            .iter()
            .map(|range| (range.clone(), Mark::Code))
            .chain(self.bold.iter().map(|range| (range.clone(), Mark::Bold)))
            .chain(
                self.italic
                    .iter()
                    .map(|range| (range.clone(), Mark::Italic)),
            )
            .chain(
                self.links
                    .iter()
                    .map(|link| (link.range.clone(), Mark::Link)),
            )
            .collect();
        ranges.sort_by_key(|(range, _)| range.start);
        ranges.into_iter()
    }
}

/// The lines of the last body drawn, so a repaint draws from prepared lines
/// instead of parsing the body again.
#[derive(Default)]
pub(crate) struct Prepared(RefCell<Option<(String, Rc<[Line]>)>>);

impl Prepared {
    pub(crate) fn lines(&self, body: &str) -> Rc<[Line]> {
        let mut cached = self.0.borrow_mut();
        if let Some((drawn, lines)) = cached.as_ref()
            && drawn == body
        {
            return lines.clone();
        }
        let lines: Rc<[Line]> = parse(body).into();
        *cached = Some((body.to_owned(), lines.clone()));
        lines
    }
}

/// Prepared daemon text for the announcement card and the release notes sheet.
#[derive(Default)]
pub(crate) struct DaemonText {
    pub announcement: Prepared,
    pub release_notes: Prepared,
}

/// How much of a body is drawn.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Limits {
    /// Bytes scanned; a longer body is cut at a character boundary.
    pub(crate) body: usize,
    pub(crate) lines: usize,
    /// Characters kept of one line.
    pub(crate) line: usize,
}

impl Limits {
    /// The start of `text` scanned: at most `body` bytes, cut at a
    /// character boundary.
    pub(crate) fn clip(self, text: &str) -> &str {
        &text[..text.floor_char_boundary(self.body)]
    }
}

/// The limits for daemon text: release notes and announcements.
const DAEMON: Limits = Limits {
    body: MAX_BODY,
    lines: MAX_LINES,
    line: MAX_LINE,
};

/// Bounded, display-safe lines of a daemon-provided body.
pub(crate) fn parse(body: &str) -> Vec<Line> {
    parse_within(body, DAEMON).0
}

/// A quoted line saying something about the text around it, such as that
/// part of it is not shown.
pub(crate) fn note(text: &str) -> Line {
    Line::plain(Kind::Quote, text.to_owned())
}

/// [`parse`] within `limits`, and whether part of the body was left out.
pub(crate) fn parse_within(body: &str, limits: Limits) -> (Vec<Line>, bool) {
    let scanned = limits.clip(body);
    let mut cut = scanned.len() < body.len() || scanned.lines().count() > limits.lines;
    let mut lines = Vec::new();
    let mut fenced = false;
    for raw in scanned.lines().take(limits.lines) {
        cut |= raw.chars().count() > limits.line;
        let trimmed = raw.trim_end();
        if trimmed.trim_start().starts_with("```") {
            fenced = !fenced;
            continue;
        }
        if fenced {
            lines.push(Line::plain(Kind::Code, safe_text(trimmed, limits.line)));
            continue;
        }
        let indent = list_indent(trimmed);
        let trimmed = trimmed.trim_start();
        if trimmed.is_empty() {
            // Runs of blank lines read as one paragraph break.
            if lines
                .last()
                .is_some_and(|line: &Line| line.kind != Kind::Blank)
            {
                lines.push(Line::plain(Kind::Blank, String::new()));
            }
            continue;
        }
        if is_rule(trimmed) {
            lines.push(Line::plain(Kind::Rule, String::new()));
            continue;
        }
        if let Some(cells) = table_row(trimmed) {
            // The `|---|---|` row under a header only aligns columns.
            if !cells.iter().all(|cell| is_alignment(cell)) {
                let row = cells.iter().map(|cell| cell.trim()).collect::<Vec<_>>();
                lines.push(Line::plain(
                    Kind::Table,
                    safe_text(&row.join(" \u{2502} "), limits.line),
                ));
            }
            continue;
        }
        let heading = trimmed.trim_start_matches('#');
        let (kind, rest) = if heading.len() < trimmed.len() && heading.starts_with(' ') {
            (Kind::Heading, heading.trim())
        } else if let Some(rest) = trimmed
            .strip_prefix("- ")
            .or_else(|| trimmed.strip_prefix("* "))
            .or_else(|| trimmed.strip_prefix("+ "))
        {
            (Kind::Bullet, rest)
        } else if let Some((number, rest)) = numbered(trimmed) {
            (Kind::Numbered(number), rest)
        } else if let Some(rest) = trimmed.strip_prefix('>') {
            (Kind::Quote, rest.trim_start())
        } else {
            (Kind::Paragraph, trimmed)
        };
        let safe = safe_text(rest, limits.line);
        let mut line = if kind == Kind::Heading {
            Line::plain(kind, safe)
        } else {
            inline(kind, &safe)
        };
        if matches!(kind, Kind::Bullet | Kind::Numbered(_)) {
            line.indent = indent;
        }
        lines.push(line);
    }
    while lines.last().is_some_and(|line| line.kind == Kind::Blank) {
        lines.pop();
    }
    (lines, cut)
}

/// How deep a list item sits: two leading spaces, or a tab, per level.
fn list_indent(line: &str) -> u8 {
    let spaces: usize = line
        .chars()
        .take_while(|c| c.is_whitespace())
        .map(|c| if c == '\t' { 2 } else { 1 })
        .sum();
    u8::try_from((spaces / 2).min(MAX_INDENT)).unwrap_or(0)
}

/// `---`, `***`, or `___`, with optional spaces between.
fn is_rule(line: &str) -> bool {
    let marks: String = line.chars().filter(|c| !c.is_whitespace()).collect();
    marks.len() >= 3
        && ['-', '*', '_']
            .iter()
            .any(|mark| marks.chars().all(|c| c == *mark))
}

/// A table row's cells, when the line is one.
fn table_row(line: &str) -> Option<Vec<&str>> {
    let inner = line.strip_prefix('|')?;
    let inner = inner.strip_suffix('|').unwrap_or(inner);
    Some(inner.split('|').collect())
}

fn is_alignment(cell: &str) -> bool {
    let cell = cell.trim();
    cell.contains('-') && cell.chars().all(|c| matches!(c, '-' | ':'))
}

/// `12. item` or `12) item`: its number and text.
fn numbered(line: &str) -> Option<(u32, &str)> {
    let digits = line.find(|c: char| !c.is_ascii_digit())?;
    if digits == 0 || digits > 9 {
        return None;
    }
    let rest = line[digits..]
        .strip_prefix(". ")
        .or_else(|| line[digits..].strip_prefix(") "))?;
    Some((line[..digits].parse().ok()?, rest))
}

/// Inline code spans, `**strong**` and `*emphasized*` text, `[label](url)`
/// links, and bare URLs.
fn inline(kind: Kind, text: &str) -> Line {
    let mut line = Line::plain(kind, String::with_capacity(text.len()));
    let mut rest = text;
    while !rest.is_empty() {
        if let Some(after) = rest.strip_prefix('`')
            && let Some(close) = after.find('`')
        {
            let start = line.text.len();
            line.text.push_str(&after[..close]);
            line.code.push(start..line.text.len());
            rest = &after[close + 1..];
            continue;
        }
        if let Some(after) = rest.strip_prefix("**")
            && let Some(close) = after.find("**")
            && flanked(&after[..close])
        {
            let start = line.text.len();
            line.text.push_str(&after[..close]);
            line.bold.push(start..line.text.len());
            rest = &after[close + 2..];
            continue;
        }
        // Emphasis opens only after a non-word character, so `snake_case`
        // and `2*3*4` stay literal.
        let opens = line
            .text
            .chars()
            .next_back()
            .is_none_or(|c| !c.is_alphanumeric());
        if opens
            && let Some(marker) = rest.chars().next().filter(|c| matches!(c, '*' | '_'))
            && let Some(after) = rest.strip_prefix(marker)
            && !after.starts_with(marker)
            && let Some(close) = after.find(marker)
            && flanked(&after[..close])
            && after[close + 1..]
                .chars()
                .next()
                .is_none_or(|c| !c.is_alphanumeric())
        {
            let start = line.text.len();
            line.text.push_str(&after[..close]);
            line.italic.push(start..line.text.len());
            rest = &after[close + 1..];
            continue;
        }
        if let Some((label, url, consumed)) = markdown_link(rest) {
            let start = line.text.len();
            line.text.push_str(label);
            line.links.push(Link {
                range: start..line.text.len(),
                url: url.into(),
            });
            rest = &rest[consumed..];
            continue;
        }
        if let Some(url) = bare_url(rest) {
            let start = line.text.len();
            line.text.push_str(url);
            line.links.push(Link {
                range: start..line.text.len(),
                url: url.into(),
            });
            rest = &rest[url.len()..];
            continue;
        }
        let Some(next) = rest.chars().next() else {
            break;
        };
        line.text.push(next);
        rest = &rest[next.len_utf8()..];
    }
    line
}

/// Strong text hugs its markers, so stray asterisks stay literal.
fn flanked(content: &str) -> bool {
    !content.is_empty()
        && !content.starts_with(|c: char| c.is_whitespace() || c == '*')
        && !content.ends_with(char::is_whitespace)
}

/// `[label](url)` at the start of `text`: its label, URL, and length in bytes.
fn markdown_link(text: &str) -> Option<(&str, &str, usize)> {
    let after = text.strip_prefix('[')?;
    let close = after.find("](")?;
    let label = &after[..close];
    if label.is_empty() || label.contains(['[', ']']) {
        return None;
    }
    let target = &after[close + 2..];
    let end = target.find(')')?;
    let url = valid_url(&target[..end])?;
    Some((label, url, 1 + close + 2 + end + 1))
}

/// A bare URL at the start of `text`, without trailing sentence punctuation.
pub(crate) fn bare_url(text: &str) -> Option<&str> {
    if !(text.starts_with("https://") || text.starts_with("http://")) {
        return None;
    }
    let end = text.find(char::is_whitespace).unwrap_or(text.len());
    let url = text[..end].trim_end_matches(['.', ',', ';', ':', '!', '?', ')', '\'', '"', '>']);
    valid_url(url)
}

/// Only web links open, and only ones a browser can take verbatim.
fn valid_url(url: &str) -> Option<&str> {
    let host = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))?;
    (url.len() <= MAX_URL
        && !host.is_empty()
        && !host.starts_with(['/', '?', '#'])
        && !url.chars().any(|c| c.is_whitespace() || c.is_control()))
    .then_some(url)
}

/// Draws prepared lines. Each link opens in the browser only when clicked.
pub(crate) fn render(id: &'static str, lines: &[Line], theme: &Theme, mono: &FontConfig) -> Div {
    div().flex().flex_col().gap(px(2.)).min_w_0().children(
        lines
            .iter()
            .enumerate()
            .map(|(index, line)| self::line(id, index, line, theme, mono)),
    )
}

/// Draws one prepared line, for views that place each line themselves.
pub(crate) fn line(
    id: &'static str,
    index: usize,
    line: &Line,
    theme: &Theme,
    mono: &FontConfig,
) -> Div {
    let accent = theme.ink(theme.palette[4]);
    let content = match line.kind {
        Kind::Heading | Kind::Code | Kind::Table => line.text.clone().into_any_element(),
        _ => rich(id, index, line, theme, accent),
    };
    line_with(id, index, line, theme, mono, content)
}

/// The same line layout with caller-owned text interaction, such as transcript
/// selection. Parsing, indentation and block styling remain shared.
pub(crate) fn line_with(
    id: &'static str,
    index: usize,
    line: &Line,
    theme: &Theme,
    mono: &FontConfig,
    content: AnyElement,
) -> Div {
    let accent = theme.ink(theme.palette[4]);
    let row = div()
        .debug_selector(move || format!("{id}-line-{index}"))
        .min_w_0()
        .pl(px(f32::from(line.indent) * 14.));
    let item = |row: Div, marker: String, content: AnyElement| {
        row.flex()
            .gap(px(6.))
            .child(div().flex_none().text_color(rgb(accent)).child(marker))
            .child(div().flex_1().min_w_0().child(content))
    };
    match line.kind {
        Kind::Blank => row.h(px(6.)),
        Kind::Heading => row
            .pt(px(4.))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(rgb(theme.primary()))
            .child(content),
        Kind::Code => row
            .px(px(8.))
            .bg(rgb(theme.active))
            .text_font(mono)
            .whitespace_nowrap()
            .overflow_hidden()
            .child(content),
        Kind::Bullet => item(row, "\u{2022}".to_owned(), content),
        Kind::Numbered(number) => item(row, format!("{number}."), content),
        Kind::Quote => row
            .pl(px(8.))
            .border_l_2()
            .border_color(rgb(theme.active))
            .text_color(rgb(theme.subtext()))
            .child(content),
        Kind::Rule => row.py(px(4.)).child(div().h(px(1.)).bg(rgb(theme.active))),
        Kind::Table => row
            .px(px(8.))
            .text_font(mono)
            .whitespace_nowrap()
            .overflow_hidden()
            .child(content),
        Kind::Paragraph => row.child(content),
    }
}

fn rich(id: &'static str, index: usize, line: &Line, theme: &Theme, accent: u32) -> AnyElement {
    let text =
        StyledText::new(line.text.clone()).with_highlights(line_highlights(line, theme, accent));
    if line.links.is_empty() {
        return text.into_any_element();
    }
    let urls: Vec<String> = line.links.iter().map(|item| item.url.clone()).collect();
    InteractiveText::new((id, index), text)
        .on_click(
            line.links.iter().map(|item| item.range.clone()).collect(),
            move |clicked, _, cx| {
                if let Some(url) = urls.get(clicked) {
                    cx.open_url(url);
                }
            },
        )
        .into_any_element()
}

pub(crate) fn line_highlights(
    line: &Line,
    theme: &Theme,
    accent: u32,
) -> Vec<(Range<usize>, HighlightStyle)> {
    let code = HighlightStyle {
        color: Some(rgb(theme.foreground).into()),
        background_color: Some(rgb(theme.active).into()),
        ..Default::default()
    };
    let bold = HighlightStyle {
        font_weight: Some(FontWeight::SEMIBOLD),
        ..Default::default()
    };
    let italic = HighlightStyle {
        font_style: Some(FontStyle::Italic),
        ..Default::default()
    };
    let link = HighlightStyle {
        color: Some(rgb(accent).into()),
        underline: Some(UnderlineStyle {
            thickness: px(1.),
            ..Default::default()
        }),
        ..Default::default()
    };
    line.highlights()
        .map(|(range, mark)| {
            let style = match mark {
                Mark::Code => code,
                Mark::Bold => bold,
                Mark::Italic => italic,
                Mark::Link => link,
            };
            (range, style)
        })
        .collect()
}

#[cfg(test)]
mod tests;
