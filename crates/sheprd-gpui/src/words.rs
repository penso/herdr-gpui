//! Word boundaries for the app's single-line text fields, which Option-Left,
//! Option-Right, Option-Backspace, and Option-Delete move and delete by, as
//! in every macOS field (Ctrl elsewhere).
//!
//! Words are Unicode word segments (UAX #29) holding a letter or digit, so
//! `foo_bar` and `1.5` stay whole while spaces and punctuation between words
//! are skipped along with the word they precede.

use unicode_segmentation::UnicodeSegmentation;

fn is_word(segment: &str) -> bool {
    segment.chars().any(char::is_alphanumeric)
}

/// The start of the word before byte `offset` of `text`.
pub(crate) fn previous_start(text: &str, offset: usize) -> usize {
    text[..offset]
        .split_word_bound_indices()
        .rev()
        .find(|(_, segment)| is_word(segment))
        .map_or(0, |(start, _)| start)
}

/// The end of the word after byte `offset` of `text`.
pub(crate) fn next_end(text: &str, offset: usize) -> usize {
    text[offset..]
        .split_word_bound_indices()
        .find(|(_, segment)| is_word(segment))
        .map_or(text.len(), |(start, segment)| {
            offset + start + segment.len()
        })
}

#[cfg(test)]
mod tests;
