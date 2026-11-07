//! Text in one row of a terminal that may name a local file, the way a
//! compiler, a test runner, `git status`, or `ls` prints one. Any word may:
//! reading the row is all this does, and whether a file is there is asked,
//! off the UI thread, once the link modifier is held over the word.

use super::super::selection::separates;
use std::{borrow::Cow, ops::Range};

/// The longest path read from a row, Linux's `PATH_MAX`.
const MAX_PATH_BYTES: usize = 4096;

/// A file path read from a row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct PlainPath<'a> {
    /// The bytes of the row it covers, a `:line` or `:line:column` suffix
    /// included.
    pub(super) range: Range<usize>,
    /// The path without that suffix, with shell escapes such as `\ ` removed.
    pub(super) path: Cow<'a, str>,
    /// The line the suffix names.
    pub(super) line: Option<u32>,
    /// Whether the path runs to the end of the row, where it may continue on
    /// the next one.
    pub(super) open: bool,
}

/// The file path in one row's `text` covering the byte at `hit`. A separator
/// after a backslash belongs to the path, as in the `/tmp/a\ b` a file drop
/// or a shell's completion prints, except in a word that starts with a
/// drive, such as `C:\Users\me`, where every backslash is part of the path.
pub(super) fn plain_path(text: &str, hit: usize) -> Option<PlainPath<'_>> {
    let mut start = 0;
    let mut end = text.len();
    let mut drive = drive_path(text);
    let mut chars = text.char_indices();
    while let Some((index, c)) = chars.next() {
        if c == '\\' && !drive {
            chars.next();
        } else if separates(c) {
            if index >= hit {
                end = index;
                break;
            }
            start = index + c.len_utf8();
            drive = drive_path(&text[start..]);
        }
    }
    // Punctuation closing a sentence, or the colon after a location, is not
    // part of the path before it, unless it is escaped.
    let mut token = &text[start..end];
    while let Some(rest) = token.strip_suffix(['.', ':', '!', '?'])
        && (drive || !rest.ends_with('\\'))
    {
        token = rest;
    }
    if hit < start || hit >= start + token.len() {
        return None;
    }
    let (located, line) = without_location(token);
    let (path, separators): (_, &[char]) = if drive {
        // A Windows name holds no colon but the drive's.
        if located.matches(':').count() > 1 {
            return None;
        }
        (Cow::Borrowed(located), &['/', '\\'])
    } else {
        if has_unescaped_colon(located) {
            return None;
        }
        (unescape(located), &['/'])
    };
    looks_like_path(&path, separators).then_some(PlainPath {
        range: start..start + token.len(),
        path,
        line,
        open: end == text.len(),
    })
}

/// Whether `word` starts with a drive and the separator after it, as in
/// `C:\` or `C:/`: a Windows path, whose backslashes separate folders.
fn drive_path(word: &str) -> bool {
    starts_with_drive(word.chars())
}

/// Whether `chars` start with a drive and the separator after it; see
/// [`drive_path`].
pub(super) fn starts_with_drive(mut chars: impl Iterator<Item = char>) -> bool {
    matches!(
        (chars.next(), chars.next(), chars.next()),
        (Some(letter), Some(':'), Some('\\' | '/')) if letter.is_ascii_alphabetic()
    )
}

/// Whether `text` holds a colon no backslash escapes, which names a scheme,
/// an `scp` host, or a drive without a path rather than a local file.
fn has_unescaped_colon(text: &str) -> bool {
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        match c {
            '\\' => {
                chars.next();
            }
            ':' => return true,
            _ => {}
        }
    }
    false
}

/// `text` with the backslash before each `shell_special` character or period
/// removed; any other backslash is part of the name.
fn unescape(text: &str) -> Cow<'_, str> {
    if !text.contains('\\') {
        return Cow::Borrowed(text);
    }
    let mut path = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\'
            && let Some(&next) = chars.peek()
            && (shell_special(next) || next == '.')
        {
            path.push(next);
            chars.next();
        } else {
            path.push(c);
        }
    }
    Cow::Owned(path)
}

/// `path` as one shell word, each `shell_special` character after a
/// backslash: what a file drop pastes, and what `plain_path` reads back. A
/// final period is escaped too, or it would read as the end of a sentence.
pub(crate) fn escape_into(path: &str, out: &mut String) {
    let mut chars = path.chars().peekable();
    while let Some(c) = chars.next() {
        if escaped(c, chars.peek().is_none()) {
            out.push('\\');
        }
        out.push(c);
    }
}

/// The bytes `escape_into` writes for `path`.
pub(crate) fn escaped_len(path: &str) -> usize {
    let mut chars = path.chars().peekable();
    let mut backslashes = 0;
    while let Some(c) = chars.next() {
        backslashes += usize::from(escaped(c, chars.peek().is_none()));
    }
    path.len() + backslashes
}

fn escaped(c: char, last: bool) -> bool {
    shell_special(c) || (c == '.' && last)
}

/// Characters a pasted path escapes: every character that ends a path here,
/// such as a comma or the narrow no-break space in macOS screenshot names,
/// every one a POSIX shell, bash, or zsh would expand or quote on, and the
/// colon, which would otherwise read as a location or a scheme.
fn shell_special(c: char) -> bool {
    separates(c)
        || matches!(
            c,
            '\\' | '$' | '&' | '*' | '?' | '!' | '#' | '~' | '^' | '=' | ':'
        )
}

/// `token` without a trailing `:line` or `:line:column`, and that line.
pub(super) fn without_location(token: &str) -> (&str, Option<u32>) {
    let mut path = token;
    let mut line = None;
    for _ in 0..2 {
        match path.rsplit_once(':') {
            Some((rest, number))
                if !rest.ends_with('\\')
                    && !number.is_empty()
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

/// Whether `path`, whose folders end at any of `separators`, names a file
/// rather than only `.`, `..`, or a home.
fn looks_like_path(path: &str, separators: &[char]) -> bool {
    if path.is_empty() || path.len() > MAX_PATH_BYTES {
        return false;
    }
    // `~user` names another account's home, which is not resolved.
    if path.starts_with('~') && path != "~" && !path.starts_with("~/") {
        return false;
    }
    let name = path
        .trim_end_matches(separators)
        .rsplit(separators)
        .next()
        .unwrap_or_default();
    !matches!(name, "" | "." | ".." | "~")
}
