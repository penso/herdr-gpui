//! The editing model the app's text fields share: grapheme-aware caret and
//! selection, UTF-16 mapping for the platform's input APIs, and replacing
//! text with or without newlines. Single-line fields drop line breaks; the
//! multi-line composer keeps them.

use std::ops::Range;
use unicode_segmentation::UnicodeSegmentation;

/// Whether replaced text keeps its newlines.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Newlines {
    /// Every line break and carriage return is dropped.
    Strip,
    /// Line endings are normalized to `\n`.
    Keep,
}

/// What replaced text becomes for `newlines`.
pub(crate) fn normalize(text: &str, newlines: Newlines) -> String {
    match newlines {
        Newlines::Strip => text
            .chars()
            .filter(|ch| !matches!(ch, '\r' | '\n'))
            .collect(),
        Newlines::Keep => {
            let mut out = String::with_capacity(text.len());
            let mut chars = text.chars().peekable();
            while let Some(ch) = chars.next() {
                match ch {
                    '\r' => {
                        out.push('\n');
                        if chars.peek() == Some(&'\n') {
                            chars.next();
                        }
                    }
                    _ => out.push(ch),
                }
            }
            out
        }
    }
}

// Internal offsets are UTF-8 boundaries; only the platform input API uses UTF-16.
#[derive(Default)]
pub(crate) struct Editing {
    pub(crate) text: String,
    pub(crate) anchor: usize,
    pub(crate) cursor: usize,
    pub(crate) marked: Option<Range<usize>>,
}

pub(crate) fn from_utf16(text: &str, offset: usize) -> usize {
    let mut units = 0;
    for (byte, ch) in text.char_indices() {
        if units >= offset {
            return byte;
        }
        units += ch.len_utf16();
    }
    text.len()
}

pub(crate) fn to_utf16(text: &str, offset: usize) -> usize {
    text[..offset].encode_utf16().count()
}

pub(crate) fn byte_range(text: &str, range: Range<usize>) -> Range<usize> {
    // Clamp out-of-bounds requests and round split surrogate pairs forward.
    from_utf16(text, range.start.min(range.end))..from_utf16(text, range.start.max(range.end))
}

impl Editing {
    pub(crate) fn selection(&self) -> Range<usize> {
        self.anchor.min(self.cursor)..self.anchor.max(self.cursor)
    }

    pub(crate) fn select_to(&mut self, offset: usize, extend: bool) {
        self.cursor = offset;
        if !extend {
            self.anchor = offset;
        }
    }

    // Grapheme boundaries of the whole text, so arrows and deletion never
    // split what reads as one character: an accented letter, an emoji
    // sequence, a Hangul syllable. A slice would start a cluster of its own
    // at the cursor, splitting a letter from the accent typed before it.
    pub(crate) fn boundaries(&self) -> impl DoubleEndedIterator<Item = usize> + '_ {
        self.text
            .grapheme_indices(true)
            .map(|(i, _)| i)
            .chain([self.text.len()])
    }

    pub(crate) fn previous(&self) -> usize {
        self.boundaries().rfind(|i| *i < self.cursor).unwrap_or(0)
    }

    pub(crate) fn next(&self) -> usize {
        self.boundaries()
            .find(|i| *i > self.cursor)
            .unwrap_or(self.cursor)
    }

    /// The first boundary at or after `offset`, where an edit leaves the
    /// caret when what it inserted joins the cluster after it.
    fn boundary_from(&self, offset: usize) -> usize {
        self.boundaries()
            .find(|i| *i >= offset)
            .unwrap_or(self.text.len())
    }

    pub(crate) fn replace(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        composing: bool,
        selection: Option<Range<usize>>,
        newlines: Newlines,
    ) -> bool {
        let range = range
            .map(|r| byte_range(&self.text, r))
            .or_else(|| self.marked.clone())
            .unwrap_or_else(|| self.selection());
        let inserted = normalize(text, newlines);
        let changed = self.text[range.clone()] != inserted;
        self.text.replace_range(range.clone(), &inserted);
        self.marked = (composing && !inserted.is_empty())
            .then_some(range.start..range.start + inserted.len());
        self.cursor = range.start + inserted.len();
        if !composing {
            self.cursor = self.boundary_from(self.cursor);
        }
        self.anchor = self.cursor;
        if composing && let Some(selection) = selection {
            // IME selection is relative to the supplied text, not the whole buffer.
            // Map through newline handling as well as UTF-16 conversion.
            let upto = |end: usize| normalize(&text[..from_utf16(text, end)], newlines).len();
            self.anchor = range.start + upto(selection.start);
            self.cursor = range.start + upto(selection.end);
        }
        changed
    }
}
