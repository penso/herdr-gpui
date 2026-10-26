use super::{Block, Fence, Inline, Kind, MathMode, Style, formula, math_input::Normalized};
use crate::chat::visual::{Source, VisualId, Visuals};
use pulldown_cmark::{Alignment, CodeBlockKind, CowStr, Event, Options, Parser, Tag, TagEnd};
use std::{collections::HashMap, ops::Range, sync::Arc};

/// Words of one rich fragment; a longer one uses the plain line renderer.
const MAX_WORDS: usize = 4_096;
/// Markdown events of one rich fragment; a longer one is not rich.
const MAX_EVENTS: usize = 16_384;
/// Rows and columns of one table; a larger table makes the fragment plain.
const MAX_TABLE_ROWS: usize = 128;
const MAX_TABLE_COLUMNS: usize = 16;
/// Code blocks per fragment highlighted by a worker; later ones stay plain.
const MAX_HIGHLIGHTED: usize = 32;
/// Characters of a fence's language kept.
const MAX_LANGUAGE: usize = 64;

/// Every bounded message uses the same Markdown semantics. Ordinary prose is
/// laid out as shaped text runs; only mixed math needs individual inline atoms.
pub(in crate::chat) fn parse(text: &str, visuals: &mut Visuals) -> Option<Vec<Block>> {
    // Inline layout creates native word elements. Oversized rich fragments
    // use the existing bounded source renderer instead of building a huge tree.
    if text.split_whitespace().take(MAX_WORDS + 1).count() > MAX_WORDS {
        return None;
    }
    let display = super::math_input::protect_display(&normalize_math(text));
    let options = Options::ENABLE_TABLES
        | Options::ENABLE_MATH
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TASKLISTS;
    let events: Vec<_> = Parser::new_ext(&display.markdown, options)
        .into_offset_iter()
        .take(MAX_EVENTS + 1)
        .collect();
    if events.len() > MAX_EVENTS {
        return None;
    }
    let mut builder = Builder::new(&display.markdown, &display.sources, visuals);
    for (event, range) in events {
        builder.event(event, range)?;
    }
    builder.flush();
    Some(builder.blocks)
}

/// A code block whose text is still arriving.
struct OpenCode {
    language: String,
    source: String,
    /// Whether its fence is closed in the source, so it may be highlighted.
    closed: bool,
}

/// A link whose label is still arriving.
struct OpenLink {
    /// The enclosing link's target, restored when this one ends.
    previous: Option<String>,
    /// A destination that cannot be opened, written out after the label so
    /// it stays visible.
    literal: Option<String>,
}

/// An inline style that nests, counted so an inner close keeps it on.
#[derive(Clone, Copy)]
enum Mark {
    Bold,
    Italic,
    Strike,
}

/// The blocks parsed so far and the Markdown containers open around the
/// next event.
struct Builder<'a> {
    /// The protected Markdown event ranges index.
    markdown: &'a str,
    /// Display math sources by the offset of their placeholder.
    display: &'a HashMap<usize, String>,
    visuals: &'a mut Visuals,
    blocks: Vec<Block>,
    /// The paragraph being built.
    content: Vec<Inline>,
    kind: Kind,
    style: Style,
    bold: usize,
    italic: usize,
    strike: usize,
    links: Vec<OpenLink>,
    /// Open lists, each with the number of its next item when ordered.
    lists: Vec<Option<u64>>,
    quote: usize,
    /// The kind of the list item open, which its paragraphs take.
    item: Option<Kind>,
    table: Option<Vec<Alignment>>,
    rows: Vec<Vec<Vec<Inline>>>,
    row: Vec<Vec<Inline>>,
    code: Option<OpenCode>,
    /// How deep inside an image's alt text the parser is; its events are
    /// already shown as the image's source.
    image: usize,
    maths: usize,
    codes: usize,
}

impl<'a> Builder<'a> {
    fn new(
        markdown: &'a str,
        display: &'a HashMap<usize, String>,
        visuals: &'a mut Visuals,
    ) -> Self {
        Self {
            markdown,
            display,
            visuals,
            blocks: Vec::new(),
            content: Vec::new(),
            kind: Kind::Paragraph,
            style: Style::default(),
            bold: 0,
            italic: 0,
            strike: 0,
            links: Vec::new(),
            lists: Vec::new(),
            quote: 0,
            item: None,
            table: None,
            rows: Vec::new(),
            row: Vec::new(),
            code: None,
            image: 0,
            maths: 0,
            codes: 0,
        }
    }

    /// Ends the paragraph being built, if it has any content.
    fn flush(&mut self) {
        if !self.content.is_empty() {
            self.blocks.push(Block::Paragraph {
                kind: self.kind,
                indent: self.lists.len().saturating_sub(1),
                content: std::mem::take(&mut self.content),
            });
        }
    }

    fn text(&mut self, text: &str) {
        push_text(&mut self.content, text, &self.style);
    }

    /// Takes one event; `None` when the fragment cannot be rich after all.
    fn event(&mut self, event: Event<'_>, range: Range<usize>) -> Option<()> {
        if self.image > 0 {
            match event {
                Event::Start(Tag::Image { .. }) => self.image += 1,
                Event::End(TagEnd::Image) => self.image -= 1,
                _ => {}
            }
            return Some(());
        }
        if let Some(code) = &mut self.code {
            match event {
                Event::End(TagEnd::CodeBlock) => self.end_code(),
                Event::Text(text) => code.source.push_str(&text),
                _ => {}
            }
            return Some(());
        }
        match event {
            Event::Start(tag) => self.start(tag, range),
            Event::End(tag) => return self.end(tag),
            Event::InlineMath(source) => {
                match formula(&source, MathMode::Inline, &mut self.maths, self.visuals) {
                    Some((id, source)) => self.content.push(Inline::Math { id, source }),
                    None => self.text(&format!("${source}$")),
                }
            }
            Event::DisplayMath(source) => self.display_math(&source, range),
            Event::Text(text) => push_linked_text(&mut self.content, &text, &self.style),
            Event::Html(text) | Event::InlineHtml(text) => self.text(&text),
            Event::Code(text) => {
                let mut style = self.style.clone();
                style.code = true;
                push_text(
                    &mut self.content,
                    &text.replace('\t', &" ".repeat(super::TAB_WIDTH)),
                    &style,
                );
            }
            Event::SoftBreak => self.text(" "),
            Event::HardBreak => self.content.push(Inline::Break),
            Event::TaskListMarker(done) => self.text(if done { "☑ " } else { "☐ " }),
            Event::Rule => {
                self.flush();
                self.blocks.push(Block::Paragraph {
                    kind: Kind::Rule,
                    indent: 0,
                    content: Vec::new(),
                });
            }
            _ => {}
        }
        Some(())
    }

    fn start(&mut self, tag: Tag<'_>, range: Range<usize>) {
        match tag {
            // Standalone supported pictures are handled by the message layer.
            // Remaining inline/unsupported images stay exact visible source;
            // rendering must neither discard their path nor load it implicitly.
            Tag::Image { .. } => {
                let markdown = self.markdown;
                self.text(&markdown[range]);
                self.image = 1;
            }
            Tag::CodeBlock(info) => {
                self.flush();
                let (language, closed) = match info {
                    CodeBlockKind::Indented => (String::new(), true),
                    CodeBlockKind::Fenced(info) => (
                        info.split_whitespace()
                            .next()
                            .unwrap_or("")
                            .chars()
                            .take(MAX_LANGUAGE)
                            .collect::<String>()
                            .to_ascii_lowercase(),
                        closed_fence(&self.markdown[range], self.quote),
                    ),
                };
                self.code = Some(OpenCode {
                    language,
                    source: String::new(),
                    closed,
                });
            }
            Tag::Paragraph => {
                self.kind = self.item.unwrap_or(if self.quote > 0 {
                    Kind::Quote
                } else {
                    Kind::Paragraph
                });
            }
            Tag::Heading { level, .. } => {
                self.flush();
                self.kind = Kind::Heading(level);
            }
            Tag::BlockQuote(_) => {
                self.quote += 1;
                self.kind = Kind::Quote;
            }
            Tag::List(start) => {
                self.flush();
                self.lists.push(start);
            }
            Tag::Item => {
                self.flush();
                self.kind = match self.lists.last_mut() {
                    Some(Some(number)) => {
                        let n = *number;
                        *number = number.saturating_add(1);
                        Kind::Numbered(n.min(u32::MAX as u64) as u32)
                    }
                    _ => Kind::Bullet,
                };
                self.item = Some(self.kind);
            }
            Tag::Strong => self.mark(Mark::Bold, true),
            Tag::Emphasis => self.mark(Mark::Italic, true),
            Tag::Strikethrough => self.mark(Mark::Strike, true),
            Tag::Link { dest_url, .. } => self.start_link(dest_url),
            Tag::Table(align) => {
                self.flush();
                self.table = Some(align);
            }
            Tag::TableHead | Tag::TableRow => self.row.clear(),
            _ => {}
        }
    }

    /// Ends a container; `None` when a table outgrows its bounds.
    fn end(&mut self, tag: TagEnd) -> Option<()> {
        match tag {
            TagEnd::Paragraph | TagEnd::Heading(_) | TagEnd::Item => {
                self.flush();
                self.item = None;
                self.kind = Kind::Paragraph;
            }
            TagEnd::BlockQuote(_) => {
                self.flush();
                self.quote = self.quote.saturating_sub(1);
            }
            TagEnd::List(_) => {
                self.lists.pop();
            }
            TagEnd::Strong => self.mark(Mark::Bold, false),
            TagEnd::Emphasis => self.mark(Mark::Italic, false),
            TagEnd::Strikethrough => self.mark(Mark::Strike, false),
            TagEnd::Link => {
                if let Some(OpenLink { previous, literal }) = self.links.pop() {
                    if let Some(target) = literal {
                        self.text(&format!("]({target})"));
                    }
                    self.style.link = previous;
                }
            }
            TagEnd::TableCell => self.row.push(std::mem::take(&mut self.content)),
            TagEnd::TableHead | TagEnd::TableRow => self.rows.push(std::mem::take(&mut self.row)),
            TagEnd::Table => {
                if self.rows.len() > MAX_TABLE_ROWS
                    || self.rows.iter().any(|row| row.len() > MAX_TABLE_COLUMNS)
                {
                    return None;
                }
                if let Some(align) = self.table.take() {
                    self.blocks.push(Block::Table {
                        align,
                        rows: std::mem::take(&mut self.rows),
                    });
                }
            }
            _ => {}
        }
        Some(())
    }

    fn mark(&mut self, mark: Mark, open: bool) {
        let (depth, on) = match mark {
            Mark::Bold => (&mut self.bold, &mut self.style.bold),
            Mark::Italic => (&mut self.italic, &mut self.style.italic),
            Mark::Strike => (&mut self.strike, &mut self.style.strike),
        };
        *depth = if open {
            *depth + 1
        } else {
            depth.saturating_sub(1)
        };
        *on = *depth > 0;
    }

    fn start_link(&mut self, destination: CowStr<'_>) {
        let previous = self.style.link.take();
        self.style.link = crate::browser::WebUrl::try_from(destination.as_ref())
            .ok()
            .map(|url| url.as_str().to_owned())
            .or_else(|| {
                crate::terminal::file_target(&destination).map(|_| destination.to_string())
            });
        // Unsupported schemes remain visible, never actionable.
        let literal = self.style.link.is_none().then(|| destination.into_string());
        if literal.is_some() {
            self.text("[");
        }
        self.links.push(OpenLink { previous, literal });
    }

    fn end_code(&mut self) {
        let Some(OpenCode {
            language,
            source,
            closed,
        }) = self.code.take()
        else {
            return;
        };
        let source: Arc<str> = source.into();
        let id = VisualId::of(("code", &language, &source));
        if closed && source.len() <= super::MAX_CODE_BYTES && self.codes < MAX_HIGHLIGHTED {
            self.visuals.want(
                id,
                Source::Code {
                    language: language.clone(),
                    source: source.clone(),
                },
            );
            self.codes += 1;
        }
        self.blocks.push(Block::Code {
            id,
            language,
            source,
        });
    }

    fn display_math(&mut self, source: &str, range: Range<usize>) {
        let display = self.display;
        let source = display.get(&range.start).map_or(source, String::as_str);
        let Some((id, source)) = formula(source, MathMode::Display, &mut self.maths, self.visuals)
        else {
            self.text(&format!("$${source}$$"));
            return;
        };
        if self.table.is_some() {
            self.content.push(Inline::Math { id, source });
        } else {
            self.flush();
            self.blocks.push(Block::Math { id, source });
        }
    }
}

fn push_text(content: &mut Vec<Inline>, text: &str, style: &Style) {
    // The fragment is already bounded by message(). Keep line breaks in
    // literal image/HTML source; stripping them can concatenate its tokens.
    let text: String = text
        .chars()
        .filter(|ch| *ch == '\n' || !crate::notifications::unsafe_char(*ch))
        .collect();
    if let Some(Inline::Text(previous, previous_style)) = content.last_mut()
        && previous_style == style
    {
        previous.push_str(&text);
    } else {
        content.push(Inline::Text(text, style.clone()));
    }
}

/// Preserve the plain renderer's bare web links without reinterpreting code
/// or explicit link labels. Inspect each whitespace-delimited token once;
/// candidates passed to the shared detector are bounded by its URL limit.
fn push_linked_text(content: &mut Vec<Inline>, text: &str, style: &Style) {
    if style.code || style.link.is_some() {
        push_text(content, text, style);
        return;
    }
    for token in text.split_inclusive(char::is_whitespace) {
        let end = token
            .trim_end_matches(char::is_whitespace)
            .trim_end_matches(['.', ',', ';', ':', '!', '?', ')', '\'', '"', '>'])
            .len();
        let found = token[..end].match_indices("http").find_map(|(start, _)| {
            let candidate = &token[start..end];
            (candidate.len() <= crate::release_notes::MAX_URL)
                .then(|| crate::release_notes::bare_url(candidate))
                .flatten()
                .map(|url| (start, url))
        });
        if let Some((start, url)) = found {
            push_text(content, &token[..start], style);
            let mut linked = style.clone();
            linked.link = Some(url.to_owned());
            push_text(content, url, &linked);
            push_text(content, &token[start + url.len()..], style);
        } else {
            push_text(content, token, style);
        }
    }
}

/// Accept the other common TeX delimiters, without rewriting code or HTML.
/// Only complete pairs are converted, so unfinished streamed formulas remain source.
/// The code and HTML it left alone are located in the result, so the display
/// pass need not parse the Markdown again.
pub(super) fn normalize_math(text: &str) -> Normalized {
    let protected = super::math_input::protected_ranges(text);
    let mut result = String::with_capacity(text.len());
    let mut kept = Vec::with_capacity(protected.len());
    let mut cursor = 0;
    let mut ranges = protected.iter().cloned().peekable();
    // Index closers once. Searching each remaining suffix for an unmatched
    // opener is quadratic on adversarial (or unfinished streamed) prose.
    let (mut inline, mut display) = (Vec::new(), Vec::new());
    let mut chars = text.char_indices();
    while let Some((at, ch)) = chars.next() {
        if ch == '\\' {
            let inside = protected.partition_point(|region| region.start <= at);
            let protected_closer = inside
                .checked_sub(1)
                .is_some_and(|index| at < protected[index].end);
            match chars.next() {
                Some((_, ')')) if !protected_closer => inline.push(at),
                Some((_, ']')) if !protected_closer => display.push(at),
                _ => {}
            }
        }
    }
    let mut inline = inline.into_iter().peekable();
    let mut display = display.into_iter().peekable();
    while cursor < text.len() {
        if let Some(range) = ranges.peek()
            && range.start <= cursor
        {
            let end = range.end.max(cursor);
            let start = result.len();
            result.push_str(&text[cursor..end]);
            kept.push(start..result.len());
            cursor = end;
            ranges.next();
            continue;
        }
        let rest = &text[cursor..];
        if rest.starts_with("\\\\") {
            result.push_str("\\\\");
            cursor += 2;
            continue;
        }
        let pair = if rest.starts_with("\\(") {
            Some((&mut inline, "$"))
        } else if rest.starts_with("\\[") {
            Some((&mut display, "$$"))
        } else {
            None
        };
        if let Some((closers, marker)) = pair {
            while closers.peek().is_some_and(|at| *at < cursor + 2) {
                closers.next();
            }
            // pulldown-cmark opens inline `$` only before a non-space and closes
            // it only after one, so `\( x \)` must lose its inner padding.
            // Display math keeps its exact source.
            if let Some(&end) = closers.peek()
                && (marker == "$$" || ranges.peek().is_none_or(|range| end < range.start))
                && let body = match marker {
                    "$" => text[cursor + 2..end].trim(),
                    _ => &text[cursor + 2..end],
                }
                && !body.trim().is_empty()
            {
                result.push_str(marker);
                result.push_str(body);
                result.push_str(marker);
                cursor = end + 2;
                continue;
            }
        }
        let Some(ch) = rest.chars().next() else {
            break;
        };
        result.push(ch);
        cursor += ch.len_utf8();
    }
    Normalized {
        text: result,
        protected: kept,
    }
}

/// Whether a code block's source, nested `quotes` block quotes deep, ends
/// with the line that closes its fence.
fn closed_fence(source: &str, quotes: usize) -> bool {
    // Inside a container, the block's later lines keep the container's
    // indentation and quote markers, so both ends are read without them.
    let mut lines = source.lines();
    let Some(fence) = lines.next().and_then(|open| Fence::open(open.trim_start())) else {
        return false;
    };
    lines.last().is_some_and(|close| {
        let close = super::quote::strip(close, quotes).unwrap_or(close);
        fence.closes(close.trim())
    })
}
