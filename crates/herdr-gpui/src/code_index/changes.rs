//! The checkout's uncommitted changes, newest first, so a search with no
//! query starts on the files an agent is working on, as herdr-nvim's picker
//! does. Recency is each file's modification time; counts are Git's.

use std::{collections::HashMap, path::Path, time::SystemTime};

/// Changed files kept at most; a checkout with more lists its newest.
pub(super) const MAX_CHANGES: usize = 500;

/// One changed file, as an index into the index's files.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Change {
    pub(crate) file: usize,
    /// Lines added and removed since the last commit; `None` for a binary
    /// file or one Git does not track yet.
    pub(crate) counts: Option<(u32, u32)>,
    /// Untracked: new since the last commit.
    pub(crate) new: bool,
    pub(crate) modified: Option<SystemTime>,
}

/// Line counts by path from `git diff --numstat -z`: `added\tremoved\tpath`
/// records, or for a rename `added\tremoved\t` followed by the old and new
/// paths as records of their own. A binary file counts `-`.
pub(super) fn numstat(output: &[u8]) -> HashMap<String, Option<(u32, u32)>> {
    let mut counts = HashMap::new();
    let mut records = output
        .split(|byte| *byte == 0)
        .filter_map(|record| std::str::from_utf8(record).ok());
    while let Some(record) = records.next() {
        let mut fields = record.splitn(3, '\t');
        let (Some(added), Some(removed), Some(path)) =
            (fields.next(), fields.next(), fields.next())
        else {
            continue;
        };
        let path = if path.is_empty() {
            // A rename: the old path, then the new one.
            records.next();
            match records.next() {
                Some(path) => path,
                None => break,
            }
        } else {
            path
        };
        let parsed = added.parse().ok().zip(removed.parse().ok());
        counts.insert(path.to_owned(), parsed);
    }
    counts
}

/// The changes among `files`, newest first: those `counts` names, and those
/// in `untracked`. Reads each one's modification time. Blocking.
pub(super) fn collect(
    root: &Path,
    files: &[String],
    counts: &HashMap<String, Option<(u32, u32)>>,
    untracked: &[String],
) -> Vec<Change> {
    let mut changes: Vec<Change> = files
        .iter()
        .enumerate()
        .filter_map(|(file, name)| {
            let new = untracked.binary_search(name).is_ok();
            let counts = match counts.get(name) {
                Some(counts) => *counts,
                None if new => None,
                None => return None,
            };
            Some(Change {
                file,
                counts,
                new,
                modified: None,
            })
        })
        .collect();
    // A file deleted since the last commit has nothing to open.
    changes.retain_mut(|change| {
        let Ok(metadata) = std::fs::symlink_metadata(root.join(&files[change.file])) else {
            return false;
        };
        change.modified = metadata.modified().ok();
        true
    });
    changes.sort_by(|a, b| b.modified.cmp(&a.modified).then(a.file.cmp(&b.file)));
    changes.truncate(MAX_CHANGES);
    changes
}
