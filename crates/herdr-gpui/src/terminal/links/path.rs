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
    /// The bytes of the row it covers, a `:line` or `:line:column` suffix and
    /// a `?query` or `#fragment` included.
    pub(super) range: Range<usize>,
    /// The path without that suffix, with shell escapes such as `\ ` removed.
    pub(super) path: Cow<'a, str>,
    /// The line the suffix names, or else a `#L10` fragment.
    pub(super) line: Option<u32>,
    /// The `?query` or `#fragment` a page's address ends in, with its `?` or
    /// `#`; the range covers it too.
    pub(super) query: Option<&'a str>,
    /// Whether the path runs to the end of the text read, where it may
    /// continue off-screen.
    pub(super) open: bool,
}

/// The file path in one row's `text` covering the byte at `hit`. A separator
/// after a backslash belongs to the path, as in the `/tmp/a\ b` a file drop
/// or a shell's completion prints, except in a word that starts with a
/// drive, such as `C:\Users\me`, where every backslash is part of the path.
/// `windows_paths` says the pane's host writes Windows paths, whose
/// backslashes separate folders when a `?query` or `#fragment` is split off.
pub(super) fn plain_path(text: &str, hit: usize, windows_paths: bool) -> Option<PlainPath<'_>> {
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
    let (named, query) = split_suffix(token, drive || windows_paths)?;
    let (located, line) = without_location(named);
    let line = line.or_else(|| query.and_then(fragment_line));
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
        query,
        open: end == text.len(),
    })
}

/// `token` split into the path it names and the `?query` or `#fragment` a
/// page's address ends in, as in `index.html?sheet=1`, the suffix keeping its
/// leading `?` or `#`. Only an unescaped mark after a folder, not starting a
/// name, with something after it splits: a `?` before `key=` or after a name
/// with an extension, and a `#` before an anchor or a `L10` line after a
/// name with an extension, or a `page=3` after a PDF's. So `notes#1`,
/// `~/.emacs.d/#init.el#`, and `/tmp/a?b` stay whole. `drive` says whether
/// the token is a Windows path, whose backslashes separate folders rather
/// than escape. `None` when the suffix is longer than [`MAX_SUFFIX_BYTES`].
pub(super) fn split_suffix(token: &str, drive: bool) -> Option<(&str, Option<&str>)> {
    let separator = |c: char| c == '/' || (drive && c == '\\');
    let mut after_folder = false;
    let mut previous = None;
    let mut chars = token.char_indices();
    while let Some((index, c)) = chars.next() {
        if c == '\\' && !drive {
            // The escaped character is part of the name.
            previous = chars.next().map(|(_, escaped)| escaped);
            continue;
        }
        if matches!(c, '?' | '#') && after_folder && previous.is_some_and(|p| !separator(p)) {
            let (path, suffix) = token.split_at(index);
            if page_suffix(path, &suffix[1..], c == '?', drive) {
                return (suffix.len() <= MAX_SUFFIX_BYTES).then_some((path, Some(suffix)));
            }
        }
        after_folder |= separator(c);
        previous = Some(c);
    }
    Some((token, None))
}

/// The longest `?query` or `#fragment` read after a path, so a page's whole
/// address stays within the 8192 bytes a web address may have.
pub(super) const MAX_SUFFIX_BYTES: usize = 4096;

/// Whether `rest`, after the `?` (`query`) or `#` that follows `path`, reads
/// as a page's query or fragment.
fn page_suffix(path: &str, rest: &str, query: bool, drive: bool) -> bool {
    if rest.is_empty() {
        return false;
    }
    let separators: &[char] = if drive { &['/', '\\'] } else { &['/'] };
    let name = without_location(path)
        .0
        .rsplit(separators)
        .next()
        .unwrap_or_default();
    let extension = name
        .rsplit_once('.')
        .map(|(_, extension)| extension)
        .filter(|extension| {
            (1..=10).contains(&extension.len())
                && extension.bytes().all(|b| b.is_ascii_alphanumeric())
        });
    if query {
        extension.is_some() || starts_like_query(rest)
    } else {
        extension.is_some_and(|extension| {
            anchor(rest) || (extension.eq_ignore_ascii_case("pdf") && pdf_page(rest))
        })
    }
}

/// Whether `fragment` names a PDF's page, as in `page=3`.
fn pdf_page(fragment: &str) -> bool {
    fragment
        .strip_prefix("page=")
        .is_some_and(|page| !page.is_empty() && page.bytes().all(|b| b.is_ascii_digit()))
}

/// Whether `rest` starts with a `key=`.
fn starts_like_query(rest: &str) -> bool {
    let key = rest
        .bytes()
        .take_while(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'.' | b'~' | b'-'))
        .count();
    key > 0 && rest.as_bytes().get(key) == Some(&b'=')
}

/// Whether `fragment` is an anchor such as `intro`, which a line such as
/// `L10` or `L10-L20` is too.
fn anchor(fragment: &str) -> bool {
    let mut bytes = fragment.bytes();
    bytes.next().is_some_and(|b| b.is_ascii_alphabetic())
        && bytes.all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'.' | b'-'))
}

/// The line a `#L10` or `#L10-L20` fragment at the end of `suffix` names:
/// the first of the range.
pub(super) fn fragment_line(suffix: &str) -> Option<u32> {
    let (_, fragment) = suffix.split_once('#')?;
    let lines = fragment.strip_prefix('L')?;
    let number = |digits: &str| {
        (!digits.is_empty() && digits.len() <= 9 && digits.bytes().all(|b| b.is_ascii_digit()))
            .then(|| digits.parse().ok())
            .flatten()
    };
    match lines.split_once('-') {
        Some((first, last)) => {
            number(last.strip_prefix('L').unwrap_or(last))?;
            number(first)
        }
        None => number(lines),
    }
}

/// Whether `word` starts with a drive and the separator after it, as in
/// `C:\` or `C:/`: a Windows path, whose backslashes separate folders.
pub(super) fn drive_path(word: &str) -> bool {
    matches!(word.as_bytes(), [letter, b':', b'\\' | b'/', ..] if letter.is_ascii_alphabetic())
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
fn without_location(token: &str) -> (&str, Option<u32>) {
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
