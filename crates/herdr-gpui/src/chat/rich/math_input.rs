//! Protect complete display formulas from Markdown block syntax. CommonMark
//! otherwise treats a standalone `=` inside TeX as a Setext heading before
//! pulldown-cmark's inline math parser can see the delimiters.

use pulldown_cmark::{Event, Parser, Tag, TagEnd};
use std::{collections::HashMap, ops::Range};

/// Markdown whose TeX delimiters are all `$` or `$$`, with the code and HTML
/// left verbatim, located in `text` and sorted, as [`protected_ranges`] finds
/// them.
pub(super) struct Normalized {
    pub(super) text: String,
    pub(super) protected: Vec<Range<usize>>,
}

pub(super) struct DisplayInput {
    pub(super) markdown: String,
    pub(super) sources: HashMap<usize, String>,
}

/// Placeholder offsets, rather than marker text, identify formulas. A user
/// writing the same marker literally cannot alias another formula's source.
pub(super) fn protect_display(input: &Normalized) -> DisplayInput {
    let Normalized { text, protected } = input;
    let bytes = text.as_bytes();
    let mut delimiters = Vec::new();
    let (mut at, mut range) = (0, 0);
    while at < bytes.len() {
        while range < protected.len() && protected[range].end <= at {
            range += 1;
        }
        if let Some(region) = protected.get(range)
            && region.start <= at
        {
            at = region.end;
            continue;
        }
        if bytes[at] == b'\\' {
            // Only ASCII dollar/backslash escapes matter to delimiter scanning.
            at += if matches!(bytes.get(at + 1), Some(b'$' | b'\\')) {
                2
            } else {
                1
            };
        } else if bytes[at] == b'$' {
            let start = at;
            while bytes.get(at) == Some(&b'$') {
                at += 1;
            }
            if at - start == 2 {
                delimiters.push(start);
            }
        } else {
            at += 1;
        }
    }
    let mut markdown = String::with_capacity(text.len());
    let mut sources = HashMap::new();
    let mut cursor = 0;
    for pair in delimiters.chunks_exact(2) {
        let (start, end) = (pair[0], pair[1]);
        let source = &text[start + 2..end];
        if !source.contains(['\n', '\r']) || source.trim().is_empty() {
            continue;
        }
        markdown.push_str(&text[cursor..start]);
        let line_start = text[..start].rfind('\n').map_or(0, |at| at + 1);
        sources.insert(
            markdown.len(),
            formula_source(source, &text[line_start..start]),
        );
        markdown.push_str("$$HERDRDISPLAY$$");
        cursor = end + 2;
    }
    markdown.push_str(&text[cursor..]);
    DisplayInput { markdown, sources }
}

/// Block quote markers belong to Markdown, never to the TeX expression. Keep
/// actual comparison symbols and all formula whitespace after those markers.
fn formula_source(source: &str, prefix: &str) -> String {
    let quotes = super::quote::depth(prefix);
    if quotes == 0 {
        return source.to_owned();
    }
    source
        .split_inclusive('\n')
        .enumerate()
        .map(|(index, line)| match index {
            0 => line,
            _ => super::quote::strip(line, quotes).unwrap_or(line),
        })
        .collect()
}

pub(super) fn protected_ranges(text: &str) -> Vec<Range<usize>> {
    let mut protected = Vec::new();
    let mut code_start = None;
    for (event, range) in Parser::new(text).into_offset_iter() {
        match event {
            Event::Start(Tag::CodeBlock(_)) => code_start = Some(range.start),
            Event::End(TagEnd::CodeBlock) => {
                if let Some(start) = code_start.take() {
                    protected.push(start..range.end);
                }
            }
            Event::Start(Tag::Image { .. })
            | Event::Code(_)
            | Event::Html(_)
            | Event::InlineHtml(_) => protected.push(range),
            _ => {}
        }
    }
    protected.sort_unstable_by_key(|region| region.start);
    let mut merged: Vec<Range<usize>> = Vec::new();
    for region in protected {
        if let Some(previous) = merged.last_mut()
            && previous.end >= region.start
        {
            previous.end = previous.end.max(region.end);
        } else {
            merged.push(region);
        }
    }
    merged
}
