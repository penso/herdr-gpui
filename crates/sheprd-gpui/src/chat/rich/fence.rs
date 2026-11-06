//! Fenced code block lines as CommonMark reads them: the message layer
//! splits diagrams and pages out of a message by them, and the rich parser
//! asks whether a code block's fence is closed yet.

/// The line that opens a fenced code block.
#[derive(Debug, PartialEq, Eq)]
pub(in crate::chat) struct Fence<'a> {
    /// A backtick or a tilde.
    marker: char,
    /// How many markers open the fence; a closing run needs at least as many.
    count: usize,
    /// What follows the markers, such as a language.
    pub(in crate::chat) info: &'a str,
}

impl<'a> Fence<'a> {
    /// The fence `line` opens: at most three spaces, then three or more
    /// backticks or tildes. A backtick fence's info has no backtick, or the
    /// line is inline code.
    pub(in crate::chat) fn open(line: &'a str) -> Option<Self> {
        let trimmed = line.trim_start_matches(' ');
        if line.len() - trimmed.len() > 3 {
            return None;
        }
        let marker = trimmed
            .chars()
            .next()
            .filter(|ch| matches!(ch, '`' | '~'))?;
        let count = trimmed.chars().take_while(|ch| *ch == marker).count();
        if count < 3 {
            return None;
        }
        let info = trimmed[count..].trim();
        (marker != '`' || !info.contains('`')).then_some(Self {
            marker,
            count,
            info,
        })
    }

    /// Whether `line` closes this fence: at most three spaces, then at least
    /// as many of the same marker, then only spaces, tabs, or the line end.
    pub(in crate::chat) fn closes(&self, line: &str) -> bool {
        let close = line.trim_end_matches([' ', '\t', '\r', '\n']);
        let run = close.trim_start_matches(' ');
        close.len() - run.len() <= 3
            && run.len() >= self.count
            && run.chars().all(|ch| ch == self.marker)
    }
}

#[cfg(test)]
mod tests;
