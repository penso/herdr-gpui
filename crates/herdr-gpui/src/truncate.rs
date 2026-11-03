//! Cutting text the app shows to a length. Cuts fall between grapheme
//! clusters, so one never leaves half of what a reader sees as a single
//! character: a letter without its accent, part of an emoji sequence, or a
//! Hangul syllable without its final consonant.

use std::borrow::Cow;
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

pub(crate) const ELLIPSIS: char = '\u{2026}';

/// The first `limit` grapheme clusters of `text`, or `None` when it has no
/// more than that.
pub(crate) fn cut(text: &str, limit: usize) -> Option<&str> {
    text.grapheme_indices(true)
        .nth(limit)
        .map(|(end, _)| &text[..end])
}

/// The longest start of `text` made of whole grapheme clusters that holds at
/// most `chars` characters, or `None` when all of it does. One cluster can
/// carry any number of combining marks, so a count of clusters alone does
/// not bound the size of untrusted text; this does, without splitting one.
pub(crate) fn cut_chars(text: &str, chars: usize) -> Option<&str> {
    let mut used = 0;
    for (start, grapheme) in text.grapheme_indices(true) {
        used += grapheme.chars().count();
        if used > chars {
            return Some(&text[..start]);
        }
    }
    None
}

/// One line of untrusted text: controls, breaks, and runs of whitespace
/// become single spaces, direction overrides go, and it is cut to `limit` grapheme
/// clusters and at most four characters each, ending with an ellipsis when
/// anything was cut.
pub(crate) fn one_line(text: &str, limit: usize) -> String {
    let budget = limit.saturating_mul(4);
    let bounded = cut_chars(text, budget);
    let spaced: String = bounded
        .unwrap_or(text)
        .chars()
        // Breaks become spaces before `safe_text` drops line and paragraph
        // separators, so the words around them never run together.
        .map(|c| {
            if c.is_control() || c.is_whitespace() {
                ' '
            } else {
                c
            }
        })
        .collect();
    let safe = crate::notifications::safe_text(&spaced, budget);
    let joined = safe.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut line = ellipsize(&joined, limit).into_owned();
    // Text that was all breaks and controls stays empty, so a blank note is
    // still refused as saying nothing; text whose first character alone is
    // over the budget still shows that something was cut.
    let blank = || {
        text.trim_matches(|c: char| c.is_whitespace() || c.is_control())
            .is_empty()
    };
    if bounded.is_some() && !line.ends_with(ELLIPSIS) && !(line.is_empty() && blank()) {
        line.push(ELLIPSIS);
    }
    line
}

/// The start of `text` that fits in `columns` terminal columns, or `None`
/// when all of it fits. A CJK character takes two columns, so a budget set
/// in Latin letters also holds about as wide a CJK label, not twice as wide.
pub(crate) fn cut_width(text: &str, columns: usize) -> Option<&str> {
    let mut used = 0;
    for (start, grapheme) in text.grapheme_indices(true) {
        used += grapheme.width();
        if used > columns {
            return Some(&text[..start]);
        }
    }
    None
}

/// `text` cut to `limit` grapheme clusters, followed by an ellipsis when
/// anything was cut.
pub(crate) fn ellipsize(text: &str, limit: usize) -> Cow<'_, str> {
    match cut(text, limit) {
        Some(kept) => Cow::Owned(format!("{kept}{ELLIPSIS}")),
        None => Cow::Borrowed(text),
    }
}

#[cfg(test)]
mod tests;
