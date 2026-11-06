//! Keeping the entry list's measured rows in step with the transcript: a
//! streamed chunk remeasures one row, a new entry appends one, a truncation
//! drops rows from the front, and anything else starts the list over.

use sheprd_chat_protocol::EntryId;
use std::ops::Range;

/// One edit of the list: the rows in `range` become `count` new rows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Splice {
    pub(crate) range: Range<usize>,
    pub(crate) count: usize,
}

/// How to bring a list showing `old` to `new`, both entry ids with their
/// revisions; `None` when only a reset will do.
pub(crate) fn splices(old: &[(EntryId, u64)], new: &[(EntryId, u64)]) -> Option<Vec<Splice>> {
    let Some(&(first, _)) = new.first() else {
        return old.is_empty().then(Vec::new);
    };
    // Entries before the first kept one were truncated away.
    let dropped = old
        .iter()
        .position(|(id, _)| *id == first)
        .unwrap_or(old.len());
    let kept = &old[dropped..];
    if kept.len() > new.len() || kept.iter().zip(new).any(|(old, new)| old.0 != new.0) {
        return None;
    }
    let mut splices = Vec::new();
    if dropped > 0 {
        splices.push(Splice {
            range: 0..dropped,
            count: 0,
        });
    }
    splices.extend(
        kept.iter()
            .zip(new)
            .enumerate()
            .filter(|(_, (old, new))| old.1 != new.1)
            .map(|(index, _)| Splice {
                range: index..index + 1,
                count: 1,
            }),
    );
    if new.len() > kept.len() {
        splices.push(Splice {
            range: kept.len()..kept.len(),
            count: new.len() - kept.len(),
        });
    }
    Some(splices)
}
