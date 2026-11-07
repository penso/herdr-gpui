//! Staging one hunk of a checkout's changes, as `git add -p` does.
//!
//! A review shows a file against HEAD; what can be staged is the difference
//! between the index and the working tree. Both end at the working tree, so
//! a hunk is found by where its changes are in the file as it is now, and
//! what is unstaged at those places goes to the index. The diff is read
//! without context, so each change stands alone and one that only the index
//! knows, beside the hunk, stays as it is. The patch is cut from Git's
//! own bytes rather than the text the review shows, which is cleaned for
//! display. A review can be out of date when the file has moved on, so every
//! line about to be staged is first compared with what the user saw, and the
//! file with the lines shown. What the index loses is not compared: it can
//! hold an edit staged earlier that the review, against HEAD, never showed.
//! Lines gone since the review are found by reading that diff against HEAD
//! again.
//!
//! Writes run to their deadline and are never cancelled, as `git.rs`'s are
//! not. Blocking, for a background executor.

mod error;

pub use error::Error;

use crate::git::{git, git_bytes, literal};
use std::{
    collections::HashMap,
    io::{self, Read as _, Write as _},
    path::Path,
    time::{Duration, Instant},
};

const DEADLINE: Duration = Duration::from_secs(30);
/// The largest diff of one file read to find its hunks.
const MAX_DIFF: usize = 64 << 20;

/// Where a hunk's changes are in the working tree's lines: `start..end`,
/// or a point before line `start` for lines that were only removed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Span {
    pub start: u32,
    pub end: u32,
}

impl Span {
    /// Whether the two overlap or meet.
    pub fn touches(self, other: Self) -> bool {
        self.start <= other.end && other.start <= self.end
    }
}

/// Follows a hunk's lines, working out its [`Span`].
#[derive(Clone, Copy, Debug)]
pub struct Walk {
    next: u32,
    span: Option<Span>,
}

impl Walk {
    /// A hunk whose `@@` header says the working tree's side starts at
    /// `start` and runs `count` lines. With no lines there, Git names the
    /// line before.
    pub fn new(start: u32, count: u32) -> Self {
        Self {
            next: if count == 0 { start + 1 } else { start },
            span: None,
        }
    }

    /// The line number the next working tree line has.
    pub fn next(&self) -> u32 {
        self.next
    }

    pub fn context(&mut self) {
        self.next = self.next.saturating_add(1);
    }

    pub fn added(&mut self) {
        let end = self.next.saturating_add(1);
        self.grow(self.next, end);
        self.next = end;
    }

    pub fn removed(&mut self) {
        self.grow(self.next, self.next);
    }

    fn grow(&mut self, start: u32, end: u32) {
        self.span = Some(self.span.map_or(Span { start, end }, |span| Span {
            start: span.start.min(start),
            end: span.end.max(end),
        }));
    }

    /// Where the changes seen so far are.
    pub fn span(&self) -> Option<Span> {
        self.span
    }
}

/// The hunk a user asked to stage, as they saw it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hunk {
    /// The file's name as Git wrote it.
    pub path: String,
    /// Where each of its changes is, a run of added and removed lines.
    pub changes: Vec<Span>,
    /// The working tree's lines shown in the hunk, by line number, cleaned
    /// for display.
    pub lines: HashMap<u32, String>,
    /// The lines shown as removed against HEAD, cleaned the same way.
    pub removed: Vec<String>,
}

impl Hunk {
    fn covers(&self, span: Span) -> bool {
        self.changes.iter().any(|change| change.touches(span))
    }
}

/// Stages the part of `checkout`'s unstaged changes that `hunk` covers.
/// `clean` makes a line of the file read as `hunk.lines` do. Blocking.
pub fn stage_hunk(
    checkout: &str,
    hunk: &Hunk,
    clean: impl Fn(&str) -> String,
) -> crate::Result<()> {
    let deadline = Instant::now() + DEADLINE;
    let diff = read_diff(checkout, &[], &hunk.path, deadline)?;
    let patch = patch(&diff, hunk, &clean)?;
    // After the diff: the patch is what was read, and what it leaves out of
    // a changed file only these can show.
    check_removed(checkout, hunk, &clean, deadline)?;
    check_file(checkout, hunk, &clean, false)?;
    // Git takes the patch from a file: the process runner closes stdin.
    // Private to this user, as the patch holds the file's lines.
    let mut file = tempfile::NamedTempFile::new().map_err(Error::Patch)?;
    file.write_all(&patch).map_err(Error::Patch)?;
    file.flush().map_err(Error::Patch)?;
    let name = file.path().to_string_lossy();
    git(
        checkout,
        &[
            "apply",
            "--cached",
            "--unidiff-zero",
            "--whitespace=nowarn",
            "--",
            &name,
        ],
        "stage the hunk",
        deadline,
        &|| false,
    )?;
    Ok(())
}

/// Stages all of `hunk`'s file, one Git does not track yet, which is one
/// hunk of added lines, if it still holds those lines and no others.
/// Blocking.
pub fn stage_file(
    checkout: &str,
    hunk: &Hunk,
    clean: impl Fn(&str) -> String,
) -> crate::Result<()> {
    check_file(checkout, hunk, &clean, true)?;
    git(
        checkout,
        &["add", "--", &literal(&hunk.path)],
        "stage the file",
        Instant::now() + DEADLINE,
        &|| false,
    )?;
    Ok(())
}

/// One file's diff from `base`, the index when empty. Every setting that
/// shapes the output is spelled out, whatever the user configured: the patch
/// has to apply as it stands.
fn read_diff(
    checkout: &str,
    base: &[&str],
    path: &str,
    deadline: Instant,
) -> crate::Result<Vec<u8>> {
    let spec = literal(path);
    let mut args = vec!["diff"];
    args.extend_from_slice(base);
    args.extend([
        "--no-color",
        "--no-ext-diff",
        "--no-textconv",
        "--no-renames",
        "--src-prefix=a/",
        "--dst-prefix=b/",
        "--unified=0",
        "--",
        &spec,
    ]);
    git_bytes(
        checkout,
        &args,
        "read unstaged changes",
        deadline,
        &|| false,
        MAX_DIFF,
    )
}

/// Refuses a file losing a line against HEAD around `hunk` that the review
/// did not show going. With no commit yet there is nothing to lose.
fn check_removed(
    checkout: &str,
    hunk: &Hunk,
    clean: impl Fn(&str) -> String,
    deadline: Instant,
) -> crate::Result<()> {
    let verify = ["rev-parse", "--verify", "--quiet", "HEAD"];
    if git(checkout, &verify, "find HEAD", deadline, &|| false).is_err() {
        return Ok(());
    }
    let diff = read_diff(checkout, &["HEAD"], &hunk.path, deadline)?;
    let mut shown: HashMap<&str, usize> = HashMap::new();
    for line in &hunk.removed {
        *shown.entry(line).or_default() += 1;
    }
    let (_, parts) = split(&diff);
    let walked = parts.into_iter().filter_map(read_hunk);
    for part in walked.filter(|part| hunk.covers(part.span)) {
        for (_, kind, text) in &part.lines {
            if *kind != LineKind::Removed {
                continue;
            }
            let text = clean(&String::from_utf8_lossy(text));
            match shown.get_mut(text.as_str()) {
                Some(left) if *left > 0 => *left -= 1,
                _ => return Err(Error::Changed.into()),
            }
        }
    }
    Ok(())
}

/// Refuses a file that no longer reads as `hunk` showed it, or with `whole`,
/// that holds more than was shown. A hunk with no lines shown was all
/// removals, which leave nothing to compare.
fn check_file(
    checkout: &str,
    hunk: &Hunk,
    clean: impl Fn(&str) -> String,
    whole: bool,
) -> Result<(), Error> {
    if hunk.lines.is_empty() {
        return Ok(());
    }
    let contents = contents(&Path::new(checkout).join(&hunk.path)).map_err(|_| Error::Changed)?;
    let text = String::from_utf8_lossy(&contents);
    let mut rows: Vec<&str> = text.split('\n').collect();
    if rows.last() == Some(&"") {
        rows.pop();
    }
    let same = hunk.lines.iter().all(|(number, shown)| {
        let row = number
            .checked_sub(1)
            .and_then(|row| rows.get(usize::try_from(row).ok()?));
        row.is_some_and(|row| clean(row) == *shown)
    });
    if same && (!whole || rows.len() == hunk.lines.len()) {
        Ok(())
    } else {
        Err(Error::Changed)
    }
}

/// What Git would read at `path`: a link's target, or a bounded regular
/// file. Anything else, such as a FIFO, is never opened.
fn contents(path: &Path) -> io::Result<Vec<u8>> {
    let metadata = std::fs::symlink_metadata(path)?;
    if metadata.is_symlink() {
        let target = std::fs::read_link(path)?;
        return Ok(target.to_string_lossy().into_owned().into_bytes());
    }
    if !metadata.is_file() || metadata.len() > MAX_DIFF as u64 {
        return Err(io::ErrorKind::InvalidData.into());
    }
    let mut bytes = Vec::new();
    std::fs::File::open(path)?
        .take(MAX_DIFF as u64)
        .read_to_end(&mut bytes)?;
    Ok(bytes)
}

/// The patch taking `hunk`'s part of `diff`, one file's diff with its
/// header, read without context: that header and each change at one of
/// `hunk`'s, as Git wrote them. Changes left out need no renumbering: Git
/// finds each one by its old side, which does not depend on those before it.
fn patch(diff: &[u8], hunk: &Hunk, clean: impl Fn(&str) -> String) -> Result<Vec<u8>, Error> {
    let (header, parts) = split(diff);
    // A file become a link, or a link a file, is two diffs: not one hunk's.
    let files = diff.split(|byte| *byte == b'\n');
    if files.filter(|line| line.starts_with(b"diff ")).count() > 1 {
        return Err(Error::Changed);
    }
    // Only lines are staged: the review shows no change of mode.
    let mut patch: Vec<u8> = header
        .split_inclusive(|byte| *byte == b'\n')
        .filter(|line| !line.starts_with(b"old mode ") && !line.starts_with(b"new mode "))
        .flatten()
        .copied()
        .collect();
    let mut staged = false;
    for part in parts {
        let Some(walked) = read_hunk(part) else {
            continue;
        };
        if !hunk.covers(walked.span) {
            continue;
        }
        for (number, kind, text) in &walked.lines {
            let text = clean(&String::from_utf8_lossy(text));
            // What the index loses may be an edit staged earlier, which
            // the review never showed: `check_removed` looks at HEAD's.
            if *kind == LineKind::Added && hunk.lines.get(number) != Some(&text) {
                return Err(Error::Changed);
            }
        }
        patch.extend_from_slice(part);
        staged = true;
    }
    if !staged {
        return Err(Error::NothingToStage);
    }
    Ok(patch)
}

/// A diff's header, and its hunks, each with its `@@` line. Inside a hunk
/// every line starts with a space, `+`, `-` or `\`, so only a hunk can start
/// a line with `@@ `.
fn split(diff: &[u8]) -> (&[u8], Vec<&[u8]>) {
    let mut starts = Vec::new();
    let mut at = 0;
    for line in diff.split_inclusive(|byte| *byte == b'\n') {
        if line.starts_with(b"@@ ") {
            starts.push(at);
        }
        at += line.len();
    }
    let Some(&first) = starts.first() else {
        return (diff, Vec::new());
    };
    let ends = starts.iter().skip(1).copied().chain([diff.len()]);
    let parts = starts.iter().zip(ends).map(|(&s, e)| &diff[s..e]).collect();
    (&diff[..first], parts)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LineKind {
    Added,
    Removed,
}

/// A hunk's span, and its lines; a removed line's number is where it was.
struct Walked<'a> {
    span: Span,
    lines: Vec<(u32, LineKind, &'a [u8])>,
}

fn read_hunk(hunk: &[u8]) -> Option<Walked<'_>> {
    let mut rows = hunk.split_inclusive(|byte| *byte == b'\n');
    let (start, count) = new_side(rows.next()?)?;
    let mut walk = Walk::new(start, count);
    let mut lines = Vec::new();
    for row in rows {
        let (sign, text) = row.split_first()?;
        let text = text.strip_suffix(b"\n").unwrap_or(text);
        match sign {
            b'+' => {
                lines.push((walk.next(), LineKind::Added, text));
                walk.added();
            }
            b'-' => {
                lines.push((walk.next(), LineKind::Removed, text));
                walk.removed();
            }
            _ => {}
        }
    }
    Some(Walked {
        span: walk.span()?,
        lines,
    })
}

/// The `+start,count` of a `@@ -a,b +c,d @@` line; a missing count is 1.
pub fn new_side(header: &[u8]) -> Option<(u32, u32)> {
    let header = std::str::from_utf8(header.strip_prefix(b"@@ ")?).ok()?;
    let side = header.split(' ').nth(1)?.strip_prefix('+')?;
    let (start, count) = side.split_once(',').unwrap_or((side, "1"));
    Some((start.parse().ok()?, count.parse().ok()?))
}

#[cfg(test)]
mod tests;
