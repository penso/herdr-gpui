//! Local file paths in one row of terminal text, the way a compiler, a test
//! runner, or `git status` prints them. Reading the row is all this does:
//! whether the file exists is only checked, off the UI thread, once the path
//! is clicked.

use super::super::selection::separates;
use std::ops::Range;

/// The longest path read from a row, Linux's `PATH_MAX`.
const MAX_PATH_BYTES: usize = 4096;

/// A file path read from a row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct PlainPath<'a> {
    /// The bytes of the row it covers, a `:line` or `:line:column` suffix
    /// included.
    pub(super) range: Range<usize>,
    /// The path without that suffix.
    pub(super) path: &'a str,
    /// The line the suffix names.
    pub(super) line: Option<u32>,
    /// Whether the path runs to the end of the row, where it may continue on
    /// the next one.
    pub(super) open: bool,
}

/// The file path in one row's `text` covering the byte at `hit`.
pub(super) fn plain_path(text: &str, hit: usize) -> Option<PlainPath<'_>> {
    if text.get(hit..)?.chars().next().is_none_or(separates) {
        return None;
    }
    let start = text[..hit]
        .char_indices()
        .rev()
        .find(|(_, c)| separates(*c))
        .map_or(0, |(index, c)| index + c.len_utf8());
    let end = text[hit..].find(separates).map_or(text.len(), |i| hit + i);
    // Punctuation closing a sentence, or the colon after a location, is not
    // part of the path before it.
    let token = text[start..end].trim_end_matches(['.', ':', '!', '?']);
    if hit >= start + token.len() {
        return None;
    }
    let (path, line) = without_location(token);
    // A bare name such as `README.md` or `v1.2` reads as a path only when
    // a location says so, as in `main.rs:12`.
    if !path.contains('/') && path.len() == token.len() {
        return None;
    }
    looks_like_path(path).then_some(PlainPath {
        range: start..start + token.len(),
        path,
        line,
        open: end == text.len(),
    })
}

/// `token` without a trailing `:line` or `:line:column`, and that line.
fn without_location(token: &str) -> (&str, Option<u32>) {
    let mut path = token;
    let mut line = None;
    for _ in 0..2 {
        match path.rsplit_once(':') {
            Some((rest, number))
                if !number.is_empty()
                    && number.len() <= 9
                    && number.bytes().all(|b| b.is_ascii_digit()) =>
            {
                path = rest;
                // The leftmost number is the line; a column follows it.
                line = number.parse().ok();
            }
            _ => break,
        }
    }
    (path, line)
}

fn looks_like_path(path: &str) -> bool {
    if path.is_empty() || path.len() > MAX_PATH_BYTES {
        return false;
    }
    // A scheme, an `scp` host, or a drive letter is not a local path here.
    if path.contains(':') {
        return false;
    }
    // `~user` names another account's home, which is not resolved.
    if path.starts_with('~') && path != "~" && !path.starts_with("~/") {
        return false;
    }
    let name = path
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .unwrap_or_default();
    !matches!(name, "" | "." | ".." | "~")
}
