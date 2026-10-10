//! Issue and pull request bodies as the views draw them: the release notes'
//! bounded Markdown lines (headings, lists, bold, code, links), drawn as
//! native text. GitHub bodies also hold HTML that carries no text of its
//! own, such as template comments and image tags, which is dropped first.
//! Parsing happens once per body; a repaint reuses the lines.

use crate::release_notes::{self, Line};
use std::{cell::RefCell, rc::Rc};

/// Tags dropped with their brackets; their text, if any, stays.
const DROPPED: &[&str] = &[
    "a", "b", "br", "code", "details", "div", "em", "h1", "h2", "h3", "h4", "hr", "i", "kbd", "li",
    "ol", "p", "picture", "pre", "source", "span", "strong", "sub", "summary", "sup", "table",
    "tbody", "td", "th", "thead", "tr", "ul",
];
/// Tags that stand for media, kept as a word.
const MEDIA: &[(&str, &str)] = &[("img", "[image]"), ("video", "[video]")];
/// The longest tag looked at; anything longer is left as text.
const MAX_TAG: usize = 1024;

/// The lines of the last body drawn.
#[derive(Default)]
pub(crate) struct Markdown(RefCell<Option<(String, Rc<[Line]>)>>);

impl Markdown {
    pub(crate) fn lines(&self, body: &str) -> Rc<[Line]> {
        let mut cached = self.0.borrow_mut();
        if let Some((drawn, lines)) = cached.as_ref()
            && drawn == body
        {
            return lines.clone();
        }
        let lines: Rc<[Line]> = release_notes::parse(&without_html(body)).into();
        *cached = Some((body.to_owned(), lines.clone()));
        lines
    }
}

/// `body` without HTML comments and the tags above. Code is left as
/// written: fenced blocks and inline code spans, where a tag is the example.
/// A `<` that opens no known tag is text and stays.
pub(crate) fn without_html(body: &str) -> String {
    let mut out = String::with_capacity(body.len());
    let mut prose = String::new();
    let mut fenced = false;
    for line in body.split_inclusive('\n') {
        if line.trim_start().starts_with("```") {
            if !fenced {
                out.push_str(&prose_without_html(&prose));
                prose.clear();
            }
            fenced = !fenced;
            out.push_str(line);
        } else if fenced {
            out.push_str(line);
        } else {
            prose.push_str(line);
        }
    }
    out.push_str(&prose_without_html(&prose));
    out
}

/// Entities GitHub bodies use in prose, decoded; `&amp;` last, so an
/// escaped `&amp;lt;` reads `&lt;`.
const ENTITIES: &[(&str, &str)] = &[
    ("&nbsp;", " "),
    ("&lt;", "<"),
    ("&gt;", ">"),
    ("&quot;", "\""),
    ("&#39;", "'"),
    ("&amp;", "&"),
];

/// Prose text with its entities decoded.
fn push_text(out: &mut String, text: &str) {
    if !text.contains('&') {
        out.push_str(text);
        return;
    }
    let decoded = ENTITIES
        .iter()
        .fold(text.to_owned(), |text, (entity, value)| {
            text.replace(entity, value)
        });
    out.push_str(&decoded);
}

/// Text outside fenced blocks without its HTML; inline code spans stay.
fn prose_without_html(body: &str) -> String {
    let mut out = String::with_capacity(body.len());
    let mut rest = body;
    while let Some(start) = rest.find(['<', '`']) {
        push_text(&mut out, &rest[..start]);
        let tail = &rest[start..];
        if let Some(code) = tail.strip_prefix('`') {
            // A span runs to the next backtick; an unclosed one is text.
            match code.find('`') {
                Some(end) => {
                    out.push_str(&tail[..end + 2]);
                    rest = &code[end + 1..];
                }
                None => {
                    out.push('`');
                    rest = code;
                }
            }
            continue;
        }
        if let Some(comment) = tail.strip_prefix("<!--") {
            rest = comment.find("-->").map_or("", |end| &comment[end + 3..]);
            continue;
        }
        let Some(end) = tail.find('>').filter(|end| *end <= MAX_TAG) else {
            out.push('<');
            rest = &tail[1..];
            continue;
        };
        let name: String = tail[1..end]
            .trim_start_matches('/')
            .chars()
            .take_while(char::is_ascii_alphanumeric)
            .collect::<String>()
            .to_ascii_lowercase();
        if let Some((_, word)) = MEDIA.iter().find(|(tag, _)| *tag == name) {
            out.push_str(word);
        } else if name == "br" {
            out.push('\n');
        } else if !DROPPED.contains(&name.as_str()) {
            out.push('<');
            rest = &tail[1..];
            continue;
        }
        rest = &tail[end + 1..];
    }
    push_text(&mut out, rest);
    out
}
