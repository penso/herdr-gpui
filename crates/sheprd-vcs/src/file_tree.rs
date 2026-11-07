//! A checkout's files, as the file tree tab lists them, and how Git sees
//! each one. The listing is the code index's: bounded, cancellable, and
//! without what `.gitignore` ignores, but nothing is scanned, so it stays
//! quick on a large checkout. Blocking, for a background executor.

use crate::code_index::{self, Error, MAX_FILES};
use std::{
    cmp::Ordering,
    collections::HashMap,
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering as Atomic},
};

/// How a file differs from the last commit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Change {
    Added,
    /// Left unmerged by a merge, rebase, or cherry-pick.
    Conflicted,
    Deleted,
    Modified,
    Untracked,
}

/// One listed file. A deleted one is listed too, though it has nothing to
/// open, so what an agent removed still shows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub path: String,
    pub change: Option<Change>,
}

/// A checkout's files, folders before files at every level.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Listing {
    root: PathBuf,
    entries: Vec<Entry>,
    /// A limit left files out.
    truncated: bool,
}

impl Listing {
    /// Lists the checkout at `root`. Blocking.
    pub fn read(root: &Path, cancelled: &AtomicBool) -> crate::Result<Self> {
        let checkout = root.to_str().ok_or(Error::Root)?;
        let stop = || cancelled.load(Atomic::Acquire);
        let (tracked, untracked) = code_index::listings(checkout, &stop)?;
        // Untracked files are listed above; renames read as a deletion and
        // an addition, so each record names one file.
        let status = code_index::read_git(
            checkout,
            &[
                "status",
                "--porcelain=v1",
                "-z",
                "--untracked-files=no",
                "--no-renames",
            ],
            "read working tree state",
            &stop,
        )?;
        Ok(Self::of(root, &tracked, &untracked, &status))
    }

    /// The listing the three Git outputs describe.
    fn of(root: &Path, tracked: &[u8], untracked: &[u8], status: &[u8]) -> Self {
        let mut changes = parse_status(status);
        for name in code_index::names(untracked) {
            changes.insert(name, Change::Untracked);
        }
        let mut paths = code_index::names(tracked);
        paths.extend(code_index::names(untracked));
        // A deletion already staged has left Git's list of files.
        paths.extend(
            changes
                .iter()
                .filter(|(path, change)| **change == Change::Deleted && code_index::relative(path))
                .map(|(path, _)| path.clone()),
        );
        paths.sort_unstable_by(|a, b| tree_order(a, b));
        paths.dedup();
        let truncated = paths.len() > MAX_FILES;
        paths.truncate(MAX_FILES);
        let entries = paths
            .into_iter()
            .map(|path| Entry {
                change: changes.get(&path).copied(),
                path,
            })
            .collect();
        Self {
            root: root.to_owned(),
            entries,
            truncated,
        }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    pub fn truncated(&self) -> bool {
        self.truncated
    }

    /// A listing of `entries`, as a read would make it.
    #[cfg(any(test, feature = "test-support"))]
    pub fn fixture(root: &Path, entries: Vec<Entry>) -> Self {
        Self {
            root: root.to_owned(),
            entries,
            truncated: false,
        }
    }
}

/// The changes in `git status --porcelain=v1 -z --no-renames` output:
/// `XY path` records, X the index's state and Y the working tree's.
fn parse_status(output: &[u8]) -> HashMap<String, Change> {
    output
        .split(|byte| *byte == 0)
        .filter_map(|record| {
            let [index, tree, b' ', path @ ..] = record else {
                return None;
            };
            let path = std::str::from_utf8(path).ok()?;
            Some((path.to_owned(), change(*index, *tree)?))
        })
        .collect()
}

/// What an `XY` pair means for the file as it stands now.
fn change(index: u8, tree: u8) -> Option<Change> {
    Some(match (index, tree) {
        (b'U', _) | (_, b'U') | (b'A', b'A') | (b'D', b'D') => Change::Conflicted,
        (b'?', b'?') => Change::Untracked,
        (b'!', b'!') => return None,
        (b'D', _) | (_, b'D') => Change::Deleted,
        (b'A', _) => Change::Added,
        _ => Change::Modified,
    })
}

/// Folders before files at every level, then by name whatever its case. Two
/// paths in one folder compare equal up to it, so a folder's files stay
/// together, which the tree is built on.
fn tree_order(a: &str, b: &str) -> Ordering {
    let (mut left, mut right) = (a.split('/').peekable(), b.split('/').peekable());
    loop {
        let (Some(first), Some(second)) = (left.next(), right.next()) else {
            return a.len().cmp(&b.len());
        };
        if first == second {
            continue;
        }
        let folders = right.peek().is_some().cmp(&left.peek().is_some());
        let lower = |name: &'_ str| {
            name.chars()
                .map(|c| c.to_ascii_lowercase())
                .collect::<Vec<_>>()
        };
        return folders
            .then_with(|| lower(first).cmp(&lower(second)))
            .then_with(|| first.cmp(second));
    }
}

#[cfg(test)]
mod tests;
