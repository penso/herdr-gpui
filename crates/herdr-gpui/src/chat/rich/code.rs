//! How a code line is drawn. The view paints this text and the selection
//! document maps carets through the same offsets, so a highlight, a drawn
//! tab, and a copied range cannot disagree about where a character is.

use unicode_width::UnicodeWidthChar;

/// Lines of one code block drawn; Copy code still takes the whole block.
pub(in crate::chat) const MAX_CODE_LINES: usize = 512;
/// Characters of one code line drawn.
pub(in crate::chat) const MAX_CODE_LINE_CHARS: usize = 4_000;
/// Columns between tab stops, in code blocks and inline code alike.
pub(in crate::chat) const TAB_WIDTH: usize = 4;

/// One source line as drawn: tabs expanded to the next stop, unsafe
/// controls dropped, and at most [`MAX_CODE_LINE_CHARS`] characters kept.
#[derive(Debug)]
pub(in crate::chat) struct CodeText {
    pub(in crate::chat) text: String,
    /// Source and drawn byte offsets of each kept character, ascending in
    /// both, then the end of each.
    offsets: Vec<(usize, usize)>,
    /// Source bytes the drawn text covers; short of the line when it is cut.
    pub(in crate::chat) end: usize,
}

impl CodeText {
    pub(in crate::chat) fn expand(raw: &str) -> Self {
        let mut text = String::new();
        let mut offsets = Vec::new();
        let (mut column, mut end) = (0, 0);
        for (at, ch) in raw.char_indices().take(MAX_CODE_LINE_CHARS) {
            offsets.push((at, text.len()));
            if ch == '\t' {
                let spaces = TAB_WIDTH - column % TAB_WIDTH;
                text.extend(std::iter::repeat_n(' ', spaces));
                column += spaces;
            } else if !crate::notifications::unsafe_char(ch) {
                text.push(ch);
                column += ch.width().unwrap_or(0);
            }
            end = at + ch.len_utf8();
        }
        offsets.push((end, text.len()));
        Self { text, offsets, end }
    }

    /// Where source byte `source` is drawn, when it starts a kept character
    /// or ends the drawn text; a grammar span inside a character has none.
    pub(in crate::chat) fn shown(&self, source: usize) -> Option<usize> {
        self.offsets
            .binary_search_by_key(&source, |&(source, _)| source)
            .ok()
            .map(|index| self.offsets[index].1)
    }

    /// The source byte a drawn caret stands for. A caret inside a tab's
    /// spaces maps to the tab itself.
    pub(in crate::chat) fn source(&self, shown: usize) -> usize {
        let index = self
            .offsets
            .partition_point(|&(_, at)| at <= shown)
            .saturating_sub(1);
        self.offsets.get(index).map_or(0, |&(source, _)| source)
    }

    /// The source byte a drawn range ending at `shown` ends at, so a copy
    /// includes a whole tab or dropped control that the range reaches into.
    pub(in crate::chat) fn source_end(&self, shown: usize) -> usize {
        let index = self.offsets.partition_point(|&(_, at)| at < shown);
        self.offsets
            .get(index)
            .map_or(self.end, |&(source, _)| source)
    }
}

#[cfg(test)]
mod tests;
