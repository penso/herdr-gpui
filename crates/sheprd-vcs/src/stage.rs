//! Staging one hunk of a checkout's changes, as `git add -p` does.
//!
//! A review shows a file against HEAD; what can be staged is the difference
//! between the index and the working tree. Both end at the working tree, so
//! a hunk is found by where its changes are in the file as it is now, and
//! everything unstaged there goes to the index. The patch is cut from Git's
//! own bytes rather than the text the review shows, which is cleaned for
//! display. A review can be out of date when the file has moved on, so every
//! line about to be staged is first compared with what the user saw.
//!
//! Writes run to their deadline and are never cancelled, as `git.rs`'s are
//! not. Blocking, for a background executor.

mod error;

pub use error::Error;

use crate::git::{git, git_bytes, literal};
use std::{
    collections::HashMap,
    io::Write as _,
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
    pub span: Span,
    /// The working tree's lines shown in the hunk, by line number, cleaned
    /// for display.
    pub lines: HashMap<u32, String>,
}

/// Stages the part of `checkout`'s unstaged changes that `hunk` covers.
/// `clean` makes a line of the file read as `hunk.lines` do. Blocking.
pub fn stage_hunk(
    checkout: &str,
    hunk: &Hunk,
    clean: impl Fn(&str) -> String,
) -> crate::Result<()> {
    let deadline = Instant::now() + DEADLINE;
    let spec = literal(&hunk.path);
    // Every setting that shapes the output is spelled out, whatever the
    // user configured: the patch has to apply as it stands.
    let diff = git_bytes(
        checkout,
        &[
            "diff",
            "--no-color",
            "--no-ext-diff",
            "--no-textconv",
            "--no-renames",
            "--src-prefix=a/",
            "--dst-prefix=b/",
            "--unified=3",
            "--inter-hunk-context=0",
            "--",
            &spec,
        ],
        "read unstaged changes",
        deadline,
        &|| false,
        MAX_DIFF,
    )?;
    let patch = patch(&diff, hunk, clean)?;
    // Git takes the patch from a file: the process runner closes stdin.
    // Private to this user, as the patch holds the file's lines.
    let mut file = tempfile::NamedTempFile::new().map_err(Error::Patch)?;
    file.write_all(&patch).map_err(Error::Patch)?;
    file.flush().map_err(Error::Patch)?;
    let name = file.path().to_string_lossy();
    git(
        checkout,
        &["apply", "--cached", "--whitespace=nowarn", "--", &name],
        "stage the hunk",
        deadline,
        &|| false,
    )?;
    Ok(())
}

/// Stages all of `path`, for a file Git does not track yet, which is one
/// hunk of added lines. Blocking.
pub fn stage_file(checkout: &str, path: &str) -> crate::Result<()> {
    git(
        checkout,
        &["add", "--", &literal(path)],
        "stage the file",
        Instant::now() + DEADLINE,
        &|| false,
    )?;
    Ok(())
}

/// The patch taking `hunk`'s part of `diff`, one file's diff with its
/// header: that header and each hunk touching `hunk`'s span, as Git wrote
/// them. Hunks left out need no renumbering: Git finds each one by its old
/// side, which does not depend on the hunks before it.
fn patch(diff: &[u8], hunk: &Hunk, clean: impl Fn(&str) -> String) -> Result<Vec<u8>, Error> {
    let (header, parts) = split(diff);
    let mut patch = header.to_vec();
    let mut staged = false;
    for part in parts {
        let Some(walked) = read_hunk(part) else {
            continue;
        };
        if !walked.span.touches(hunk.span) {
            continue;
        }
        for (number, kind, text) in &walked.lines {
            let text = clean(&String::from_utf8_lossy(text));
            let shown = hunk.lines.get(number);
            let same = match kind {
                LineKind::Added => shown == Some(&text),
                // A context line can lie beyond the lines shown.
                LineKind::Context => shown.is_none_or(|shown| *shown == text),
            };
            if !same {
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
    Context,
}

/// A hunk's span, and its working tree lines by number.
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
            b' ' => {
                lines.push((walk.next(), LineKind::Context, text));
                walk.context();
            }
            b'+' => {
                lines.push((walk.next(), LineKind::Added, text));
                walk.added();
            }
            b'-' => walk.removed(),
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
