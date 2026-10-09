//! Ranking an index's symbols or files against a query, off the UI thread
//! for a large checkout. Scoring is nucleo's fzf algorithm, as the palette
//! uses; only the best hits are kept, and only they get highlights.

use crate::code_index::Index;
use nucleo_matcher::{
    Config, Matcher, Utf32Str,
    pattern::{CaseMatching, Normalization, Pattern},
};
use std::ops::Range;

/// The most rows a search lists.
pub(super) const MAX_HITS: usize = 500;
const MAX_QUERY_CHARS: usize = 256;

/// What the search lists.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Mode {
    #[default]
    Symbols,
    Files,
}

impl Mode {
    pub(super) const ALL: [Self; 2] = [Self::Symbols, Self::Files];

    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Symbols => "Symbols",
            Self::Files => "Files",
        }
    }
}

/// A query, and the line a `path:line` query names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Query {
    pub(super) text: String,
    pub(super) line: Option<u32>,
}

impl Query {
    /// Reads `text`, bounded; in Files mode a trailing `:line` is the line
    /// to open at, as an editor's quick open reads it.
    pub(super) fn parse(text: &str, mode: Mode) -> Self {
        let end = text
            .char_indices()
            .nth(MAX_QUERY_CHARS)
            .map_or(text.len(), |(start, _)| start);
        let mut text = text[..end].trim();
        let mut line = None;
        if mode == Mode::Files
            && let Some((path, number)) = text.rsplit_once(':')
            && let Ok(number) = number.parse::<u32>()
        {
            text = path;
            line = Some(number);
        }
        Self {
            text: text.split_whitespace().collect::<Vec<_>>().join(" "),
            line,
        }
    }
}

/// One listed row: a symbol or a file, by its index in the index, and the
/// byte ranges of its label the query matched.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Hit {
    pub(super) item: usize,
    pub(super) highlights: Vec<Range<usize>>,
}

/// The text a row is matched by and labelled with.
pub(super) fn label(index: &Index, mode: Mode, item: usize) -> &str {
    match mode {
        Mode::Symbols => index.symbols().get(item).map_or("", |s| s.name.as_str()),
        Mode::Files => index.files().get(item).map_or("", String::as_str),
    }
}

/// The rows an empty query lists: the uncommitted changes newest first, or
/// the definitions in them, then the rest in index order.
fn unranked(index: &Index, mode: Mode) -> Vec<usize> {
    let changed = index.changes().iter().map(|change| change.file);
    match mode {
        Mode::Files => changed
            .chain((0..index.files().len()).filter(|&file| index.change(file).is_none()))
            .take(MAX_HITS)
            .collect(),
        Mode::Symbols => {
            // Definitions are in file order, so each file's are one run.
            let symbols = index.symbols();
            let of = |file: usize| {
                symbols.partition_point(|symbol| symbol.file < file)
                    ..symbols.partition_point(|symbol| symbol.file <= file)
            };
            changed
                .flat_map(of)
                .chain(
                    (0..symbols.len()).filter(|&item| index.change(symbols[item].file).is_none()),
                )
                .take(MAX_HITS)
                .collect()
        }
    }
}

/// The best rows of `index` for `query`, best first. Ties keep the shorter
/// label first. An empty query lists [`unranked`] rows.
pub(super) fn rank(index: &Index, mode: Mode, query: &Query) -> Vec<Hit> {
    let count = match mode {
        Mode::Symbols => index.symbols().len(),
        Mode::Files => index.files().len(),
    };
    if query.text.is_empty() {
        return unranked(index, mode)
            .into_iter()
            .map(|item| Hit {
                item,
                highlights: Vec::new(),
            })
            .collect();
    }
    let mut matcher = Matcher::new(match mode {
        Mode::Symbols => Config::DEFAULT,
        Mode::Files => Config::DEFAULT.match_paths(),
    });
    let pattern = Pattern::parse(&query.text, CaseMatching::Smart, Normalization::Smart);
    let mut buffer = Vec::new();
    let mut scored: Vec<(u32, usize, usize)> = (0..count)
        .filter_map(|item| {
            let text = label(index, mode, item);
            let score = pattern.score(Utf32Str::new(text, &mut buffer), &mut matcher)?;
            Some((score, text.len(), item))
        })
        .collect();
    let order = |a: &(u32, usize, usize), b: &(u32, usize, usize)| {
        b.0.cmp(&a.0).then(a.1.cmp(&b.1)).then(a.2.cmp(&b.2))
    };
    if scored.len() > MAX_HITS {
        scored.select_nth_unstable_by(MAX_HITS, order);
        scored.truncate(MAX_HITS);
    }
    scored.sort_unstable_by(order);
    let mut indices = Vec::new();
    scored
        .into_iter()
        .map(|(_, _, item)| {
            let text = label(index, mode, item);
            indices.clear();
            pattern.indices(Utf32Str::new(text, &mut buffer), &mut matcher, &mut indices);
            Hit {
                item,
                highlights: byte_ranges(text, &mut indices),
            }
        })
        .collect()
}

/// Character indices as merged byte ranges of `text`.
fn byte_ranges(text: &str, indices: &mut Vec<u32>) -> Vec<Range<usize>> {
    indices.sort_unstable();
    indices.dedup();
    let mut ranges: Vec<Range<usize>> = Vec::new();
    let mut wanted = indices.iter().peekable();
    for (position, (start, c)) in text.char_indices().enumerate() {
        let Some(&&next) = wanted.peek() else {
            break;
        };
        if usize::try_from(next).ok() != Some(position) {
            continue;
        }
        wanted.next();
        let end = start + c.len_utf8();
        match ranges.last_mut() {
            Some(last) if last.end == start => last.end = end,
            _ => ranges.push(start..end),
        }
    }
    ranges
}
