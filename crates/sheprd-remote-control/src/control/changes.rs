//! A workspace's changes and its pull request's checks, for a phone. The
//! daemon cannot reach Sheprd, so a phone's question waits in the daemon
//! ([`Ask`]) until Sheprd takes it with `Request::ChangeAsks`, works it out
//! with its own Git, and sends `Request::ChangeAnswer`. Nothing here changes
//! a repository, and a phone names only a workspace and what Sheprd itself
//! listed, never a path to read.
//!
//! Sheprd shapes each answer within the bounds below through [`FileList`] and
//! [`FileDiff`]'s builders; the daemon checks them again ([`Answer::valid`]),
//! so a phone never receives more than these say.

use serde::{Deserialize, Serialize};

/// Files one list sends; `FileList::total` still counts them all.
pub const MAX_FILES: usize = 500;
/// The longest path sent, in bytes.
pub const MAX_PATH: usize = 1024;
/// Lines of one file's diff; a longer one is cut and says so.
pub const MAX_DIFF_LINES: usize = 3_000;
/// Characters kept of one line of a diff.
pub const MAX_LINE_CHARS: usize = 500;
/// Bytes of text in one file's diff; a larger one is cut and says so.
pub const MAX_DIFF_BYTES: usize = 512 * 1024;
/// Checks one list sends.
pub const MAX_CHECKS: usize = 100;
/// The longest check name or base branch name, in characters.
pub const MAX_NAME: usize = 256;
/// The longest link sent, in bytes.
pub const MAX_URL: usize = 2048;
/// Bytes of a failing log a phone may quote.
pub const MAX_EXCERPT: usize = sheprd_github::check_log::MAX_EXCERPT;

/// Which changes a phone looks at. Either way they end at the working tree.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Scope {
    /// What is not committed yet.
    #[default]
    Uncommitted,
    /// Everything the branch adds to its base, uncommitted work included.
    Branch,
}

/// A phone's question, named by the daemon.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AskId(pub u64);

/// A question for Sheprd about one workspace.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Ask {
    pub id: AskId,
    /// The daemon session the workspace belongs to: a workspace id means
    /// nothing in another one.
    pub boot_id: String,
    pub workspace_id: String,
    pub wanted: Wanted,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "wanted")]
pub enum Wanted {
    /// The changed files.
    Files { scope: Scope },
    /// One listed file's diff, by the path its list gave.
    Diff { scope: Scope, path: String },
    /// The checks of the workspace's pull request.
    Checks,
    /// An excerpt of a failing check's log, by the job its list gave.
    CheckLog { job: u64 },
}

/// Sheprd's answer to an [`Ask`].
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "answer", content = "body")]
pub enum Answer {
    Files(FileList),
    Diff(FileDiff),
    Checks(Checks),
    CheckLog(CheckLog),
    Failed(Failure),
}

impl Answer {
    /// Whether it stays within this module's bounds.
    pub fn valid(&self) -> bool {
        match self {
            Self::Files(list) => list.valid(),
            Self::Diff(diff) => diff.valid(),
            Self::Checks(checks) => checks.valid(),
            Self::CheckLog(log) => log.valid(),
            Self::Failed(_) => true,
        }
    }
}

/// Why Sheprd has no answer.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, thiserror::Error)]
#[serde(rename_all = "snake_case")]
pub enum Failure {
    // keep-sorted start
    #[error("this workspace is not one Sheprd has open")]
    Gone,
    #[error("no branch to compare with was found")]
    NoBase,
    #[error("that file is no longer changed")]
    NoFile,
    #[error("this check has no log to read")]
    NoLog,
    #[error("this workspace is not a Git repository Sheprd can read")]
    NoRepository,
    #[error("Sheprd could not read this")]
    #[serde(other)]
    Unreadable,
    // keep-sorted end
}

/// How a file changed.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileStatus {
    Added,
    Deleted,
    #[default]
    Modified,
    Renamed,
    Untracked,
}

/// Whether a file's diff can be shown.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Shown {
    #[default]
    Text,
    Binary,
    /// Too large to read on a phone.
    TooLarge,
    /// Git could not give its change.
    Unreadable,
}

/// One changed file.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct ChangedFile {
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub old_path: Option<String>,
    pub status: FileStatus,
    /// Absent for a binary file, or an untracked one not read yet.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub added: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub removed: Option<u32>,
    pub shown: Shown,
}

impl ChangedFile {
    fn valid(&self) -> bool {
        path(&self.path) && self.old_path.as_deref().is_none_or(path)
    }
}

fn path(text: &str) -> bool {
    !text.is_empty() && text.len() <= MAX_PATH
}

/// A workspace's changed files, the first [`MAX_FILES`] of them.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct FileList {
    /// The branch compared with, for `Scope::Branch`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base: Option<String>,
    pub files: Vec<ChangedFile>,
    /// How many files changed, listed or not.
    pub total: usize,
}

impl FileList {
    /// The list of `files`, keeping those that fit. A path too long to send
    /// still counts.
    pub fn bounded(base: Option<String>, files: impl IntoIterator<Item = ChangedFile>) -> Self {
        let mut list = Self {
            base: base.map(|base| bounded_chars(&base, MAX_NAME)),
            files: Vec::new(),
            total: 0,
        };
        for file in files {
            list.total += 1;
            if list.files.len() < MAX_FILES && file.valid() {
                list.files.push(file);
            }
        }
        list
    }

    pub fn valid(&self) -> bool {
        self.files.len() <= MAX_FILES
            && self.files.len() <= self.total
            && self.files.iter().all(ChangedFile::valid)
            && self
                .base
                .as_ref()
                .is_none_or(|base| base.chars().count() <= MAX_NAME)
    }
}

/// What a line of a diff is.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LineKind {
    Hunk,
    Context,
    Added,
    Removed,
    /// Git's remarks, such as a missing final newline.
    Meta,
}

/// One line of a unified diff.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DiffLine {
    pub kind: LineKind,
    /// Its number before the change, for context and removed lines.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub old: Option<u32>,
    /// Its number after the change, for context and added lines.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub new: Option<u32>,
    pub text: String,
}

/// One file's unified diff, cut to fit.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct FileDiff {
    pub path: String,
    pub shown: Shown,
    pub lines: Vec<DiffLine>,
    /// Lines were left out to stay within bounds, here or on the desktop.
    pub truncated: bool,
}

impl FileDiff {
    /// A diff that is only a note: binary, too large, or unreadable.
    pub fn note(path: &str, shown: Shown) -> Self {
        Self {
            path: bounded_bytes(path, MAX_PATH),
            shown,
            ..Self::default()
        }
    }

    /// The diff of `lines`, cut where it reaches [`MAX_DIFF_LINES`] or
    /// [`MAX_DIFF_BYTES`], and each line at [`MAX_LINE_CHARS`].
    pub fn bounded(path: &str, lines: impl IntoIterator<Item = DiffLine>) -> Self {
        let mut diff = Self::note(path, Shown::Text);
        let mut bytes = 0;
        for mut line in lines {
            line.text = bounded_chars(&line.text, MAX_LINE_CHARS);
            bytes += line.text.len();
            if diff.lines.len() == MAX_DIFF_LINES || bytes > MAX_DIFF_BYTES {
                diff.truncated = true;
                break;
            }
            diff.lines.push(line);
        }
        diff
    }

    pub fn valid(&self) -> bool {
        path(&self.path)
            && self.lines.len() <= MAX_DIFF_LINES
            && self.lines.iter().map(|line| line.text.len()).sum::<usize>() <= MAX_DIFF_BYTES
            && self
                .lines
                .iter()
                .all(|line| line.text.chars().count() <= MAX_LINE_CHARS)
    }
}

/// Where a check stands.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckState {
    Passed,
    Failed,
    Pending,
    Skipped,
}

impl From<sheprd_github::pull_request::Outcome> for CheckState {
    fn from(outcome: sheprd_github::pull_request::Outcome) -> Self {
        use sheprd_github::pull_request::Outcome;
        match outcome {
            Outcome::Passed => Self::Passed,
            Outcome::Failed => Self::Failed,
            Outcome::Pending => Self::Pending,
            Outcome::Skipped => Self::Skipped,
        }
    }
}

/// One check of a pull request.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Check {
    pub name: String,
    pub state: CheckState,
    /// Asks for this check's log (`Wanted::CheckLog`); only a failed GitHub
    /// Actions check has one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub job: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

impl Check {
    fn valid(&self) -> bool {
        self.name.chars().count() <= MAX_NAME
            && self.url.as_ref().is_none_or(|url| url.len() <= MAX_URL)
    }
}

/// The checks of a workspace's pull request; empty without one.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct Checks {
    /// The pull request's number, absent when the workspace has none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub number: Option<u64>,
    pub checks: Vec<Check>,
}

impl Checks {
    pub fn valid(&self) -> bool {
        self.checks.len() <= MAX_CHECKS && self.checks.iter().all(Check::valid)
    }
}

/// What a prompt quotes of a failing check's log.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CheckLog {
    pub excerpt: String,
}

impl CheckLog {
    pub fn valid(&self) -> bool {
        self.excerpt.len() <= MAX_EXCERPT
    }
}

/// At most `limit` characters of `text`, the last an ellipsis when cut.
fn bounded_chars(text: &str, limit: usize) -> String {
    match text.char_indices().nth(limit) {
        None => text.to_owned(),
        Some(_) => text
            .chars()
            .take(limit - 1)
            .chain(std::iter::once('\u{2026}'))
            .collect(),
    }
}

/// At most `limit` bytes of `text`, cut between characters.
fn bounded_bytes(text: &str, limit: usize) -> String {
    let mut end = text.len().min(limit);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    text[..end].to_owned()
}
