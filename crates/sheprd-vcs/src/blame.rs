//! Who last changed each line of a file, and in which commit, for a code
//! tab. It blames the text the tab read, not the file as it is by then, so
//! line numbers always match what is shown and a line not committed yet says
//! so. Git runs bounded and
//! cancellable; authors and summaries are untrusted text, cleaned here.
//! Blocking, for a background executor.

mod error;

pub use error::Error;

use crate::git::{git, git_bytes};
use std::{
    collections::HashMap,
    io::Write as _,
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};

const DEADLINE: Duration = Duration::from_secs(30);
/// The longest output read. A code tab shows files up to 1 MiB, and the
/// porcelain format repeats each line behind a header of about 50 bytes.
const MAX_OUTPUT: usize = 32 << 20;
/// Characters kept of an author's name and a commit's summary.
const MAX_TEXT: usize = 200;
/// The characters of a commit's ID a line shows.
const SHORT_ID: usize = 8;

/// One commit a line last changed in.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Commit {
    /// Its short ID; empty for lines not committed yet.
    pub id: String,
    pub author: String,
    /// When it was authored, in Unix seconds.
    pub time: i64,
    pub summary: String,
}

impl Commit {
    /// Whether this stands for lines not committed yet.
    pub fn uncommitted(&self) -> bool {
        self.id.is_empty()
    }
}

/// Each line's last commit, as an index into the commits.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Blame {
    commits: Vec<Commit>,
    lines: Vec<u32>,
}

impl Blame {
    /// Blames `contents`, as read from the file at `path`. Blocking.
    pub fn read(path: &Path, contents: &str, cancelled: &AtomicBool) -> crate::Result<Self> {
        let folder = path.parent().and_then(Path::to_str).ok_or(Error::Path)?;
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or(Error::Path)?;
        let stop = || cancelled.load(Ordering::Acquire);
        let deadline = Instant::now() + DEADLINE;
        // Relative to the file's folder, which may be deep in the checkout.
        let spec = format!(":(literal){name}");
        let tracked = git(
            folder,
            &["ls-files", "--", &spec],
            "check the file is tracked",
            deadline,
            &stop,
        )?;
        if tracked.is_empty() {
            return Err(Error::Untracked.into());
        }
        // Git takes the text from a file: the process runner closes stdin.
        // Private to this user, as it holds the file's lines.
        let mut file = tempfile::NamedTempFile::new().map_err(Error::Contents)?;
        file.write_all(contents.as_bytes())
            .map_err(Error::Contents)?;
        file.flush().map_err(Error::Contents)?;
        let text = file.path().to_string_lossy();
        let output = git_bytes(
            folder,
            &["blame", "--porcelain", "--contents", &text, "--", name],
            "read who changed each line",
            deadline,
            &stop,
            MAX_OUTPUT,
        )?;
        Ok(Self::parse(&output))
    }

    /// `git blame --porcelain` output: a `sha old new [count]` line for each
    /// line of the file, the commit's details the first time it appears,
    /// and the line itself behind a tab, so nothing a file holds can read as
    /// a header.
    fn parse(output: &[u8]) -> Self {
        let mut blame = Self::default();
        let mut seen: HashMap<&[u8], u32> = HashMap::new();
        let mut current = None;
        for line in output.split(|byte| *byte == b'\n') {
            if line.first() == Some(&b'\t') {
                blame.lines.extend(current);
                continue;
            }
            let mut fields = line.splitn(2, |byte| *byte == b' ');
            let (Some(key), value) = (fields.next(), fields.next().unwrap_or_default()) else {
                continue;
            };
            if commit_id(key) {
                let next = u32::try_from(blame.commits.len()).unwrap_or(u32::MAX);
                let commit = *seen.entry(key).or_insert(next);
                if commit == next {
                    blame.commits.push(Commit {
                        id: short_id(key),
                        ..Commit::default()
                    });
                }
                current = Some(commit);
                continue;
            }
            let Some(commit) = current.and_then(|commit| blame.commits.get_mut(commit as usize))
            else {
                continue;
            };
            match key {
                b"author" => commit.author = clean(value),
                b"author-time" => {
                    commit.time = std::str::from_utf8(value)
                        .ok()
                        .and_then(|time| time.parse().ok())
                        .unwrap_or(0);
                }
                b"summary" => commit.summary = clean(value),
                _ => {}
            }
        }
        blame
    }

    /// The commit 0-based line `line` last changed in.
    pub fn line(&self, line: usize) -> Option<&Commit> {
        self.commits.get(*self.lines.get(line)? as usize)
    }

    /// Whether `line` opens a run of lines from one commit.
    pub fn starts_run(&self, line: usize) -> bool {
        line == 0 || self.lines.get(line) != self.lines.get(line - 1)
    }

    pub fn len(&self) -> usize {
        self.lines.len()
    }

    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }

    /// A blame of `lines`, each naming one of `commits`.
    #[cfg(any(test, feature = "test-support"))]
    pub fn fixture(commits: Vec<Commit>, lines: Vec<u32>) -> Self {
        Self { commits, lines }
    }
}

/// A full commit ID: SHA-1's 40 hex digits or SHA-256's 64.
fn commit_id(field: &[u8]) -> bool {
    matches!(field.len(), 40 | 64) && field.iter().all(u8::is_ascii_hexdigit)
}

/// The ID lines show; Git blames uncommitted lines on an all-zero one.
fn short_id(id: &[u8]) -> String {
    if id.iter().all(|byte| *byte == b'0') {
        return String::new();
    }
    String::from_utf8_lossy(&id[..SHORT_ID]).into_owned()
}

fn clean(text: &[u8]) -> String {
    String::from_utf8_lossy(text)
        .chars()
        .filter(|c| !sheprd_client::text::unsafe_char(*c))
        .take(MAX_TEXT)
        .collect()
}

#[cfg(test)]
mod tests;
