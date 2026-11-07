//! Files dragged into the terminal. Terminals paste a drop as the files'
//! paths, quoted or backslash-escaped like shell words; a paste made only
//! of paths to existing files becomes attachments instead of text, as in
//! OpenCode's terminal view.

use std::path::Path;

/// Paths one paste attaches at most; a longer paste stays text.
const MAX_PATHS: usize = 64;

/// Whether a backslash escapes the next character. Windows terminals paste
/// a drop as plain or double-quoted paths whose separators are backslashes.
const ESCAPES: bool = cfg!(not(windows));

/// The files a paste names, when it names nothing but existing files.
pub(crate) fn files(text: &str) -> Option<Vec<String>> {
    let words = words(text.trim(), ESCAPES)?;
    let files = !words.is_empty()
        && words.len() <= MAX_PATHS
        && words.iter().all(|word| {
            let path = Path::new(word);
            path.is_absolute() && path.is_file()
        });
    files.then_some(words)
}

/// Shell words: spaces separate, a backslash escapes the next character
/// when `escapes` is set, and single or double quotes group. `None` for an
/// unclosed quote.
fn words(text: &str, escapes: bool) -> Option<Vec<String>> {
    let mut words = Vec::new();
    let mut word = String::new();
    let mut started = false;
    let mut quote = None;
    let mut chars = text.chars();
    while let Some(ch) = chars.next() {
        match (quote, ch) {
            (None, '\\') | (Some('"'), '\\') if escapes => {
                word.push(chars.next()?);
                started = true;
            }
            (None, '\'' | '"') => {
                quote = Some(ch);
                started = true;
            }
            (Some(open), _) if ch == open => quote = None,
            (None, ' ' | '\t' | '\n') => {
                if started {
                    words.push(std::mem::take(&mut word));
                    started = false;
                }
            }
            _ => {
                word.push(ch);
                started = true;
            }
        }
    }
    if quote.is_some() {
        return None;
    }
    if started {
        words.push(word);
    }
    Some(words)
}

#[cfg(test)]
mod tests;
