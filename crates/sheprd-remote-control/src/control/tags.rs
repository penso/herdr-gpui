//! The tags a person gives a computer, such as `personal` or `work`. Each
//! computer's own daemon holds them, so every phone and desktop that reaches
//! it sees the same set, and no client has to match its name for the
//! computer to anyone else's.
//!
//! A client offers its copy with the time it was edited and the daemon
//! answers with the set that stands: the latest edit wins, whole, because
//! tags are low stakes and a merge could never remove one.

use serde::{Deserialize, Serialize};

/// The most tags one computer keeps.
pub const MAX_TAGS: usize = 8;
/// The longest tag, in characters.
pub const MAX_TAG_CHARS: usize = 24;

/// A computer's tags and when they were last edited.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct ComputerTags {
    pub tags: Vec<String>,
    /// Milliseconds since the Unix epoch; 0 for tags never edited.
    pub updated_ms: u64,
}

impl ComputerTags {
    /// `tags` within bounds: single spaces, no blanks, and no repeats, as two
    /// tags differing only in case are one tag, spelled as first written.
    pub fn new<T: AsRef<str>>(tags: impl IntoIterator<Item = T>, updated_ms: u64) -> Self {
        let mut kept: Vec<String> = Vec::new();
        for tag in tags {
            let Some(tag) = normalized(tag.as_ref()) else {
                continue;
            };
            if kept.len() < MAX_TAGS && !kept.iter().any(|other| same(other, &tag)) {
                kept.push(tag);
            }
        }
        Self {
            tags: kept,
            updated_ms,
        }
    }

    /// What stands once `offer` meets these tags at `now_ms`. A clock running
    /// ahead cannot make an edit outlast every later one, so an offer counts
    /// as made by `now_ms` at the latest. On a tie the stored set stays, so
    /// offering the same tags twice changes nothing.
    #[must_use]
    pub fn merged(self, offer: Self, now_ms: u64) -> Self {
        let updated_ms = offer.updated_ms.min(now_ms);
        if updated_ms > self.updated_ms {
            Self::new(offer.tags, updated_ms)
        } else {
            self
        }
    }
}

/// `text` as a tag: single spaces, at most [`MAX_TAG_CHARS`] characters, or
/// `None` when nothing is left.
pub fn normalized(text: &str) -> Option<String> {
    let words = text
        .split_whitespace()
        .filter(|word| !word.chars().any(char::is_control))
        .collect::<Vec<_>>()
        .join(" ");
    let tag: String = words.chars().take(MAX_TAG_CHARS).collect();
    let tag = tag.trim_end();
    (!tag.is_empty()).then(|| tag.to_owned())
}

/// Whether two tags are the same tag.
pub fn same(one: &str, other: &str) -> bool {
    one.to_lowercase() == other.to_lowercase()
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests;
