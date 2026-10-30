//! The pseudo-locales' transforms. Fluent applies them to the text of each
//! message, never to its placeables, so names and numbers the app passes in
//! stay as they are, just as a real translation would leave them.
use std::borrow::Cow;

/// What an `en-XA` message starts and ends with, so text the app shows
/// without a message stands out, and a clipped label loses its closing mark.
pub const MARKS: (char, char) = ('[', ']');

/// Right-to-left override and pop: `en-XB` letters are drawn right to left
/// whichever way the layout runs, as a Hebrew word is, so text the app shows
/// without a message still reads forwards. Fluent transforms each run of a
/// message's text apart from its placeables, so a message with arguments
/// reverses piece by piece around them.
const RLO: char = '\u{202E}';
const PDF: char = '\u{202C}';

const UPPER: [char; 26] = [
    'Ȧ', 'Ɓ', 'Ƈ', 'Ḓ', 'Ḗ', 'Ƒ', 'Ɠ', 'Ħ', 'Ī', 'Ĵ', 'Ķ', 'Ŀ', 'Ḿ', 'Ƞ', 'Ǿ', 'Ƥ', 'Ɋ', 'Ř', 'Ş',
    'Ŧ', 'Ŭ', 'Ṽ', 'Ẇ', 'Ẋ', 'Ẏ', 'Ẑ',
];
const LOWER: [char; 26] = [
    'ȧ', 'ƀ', 'ƈ', 'ḓ', 'ḗ', 'ƒ', 'ɠ', 'ħ', 'ī', 'ĵ', 'ķ', 'ŀ', 'ḿ', 'ƞ', 'ǿ', 'ƥ', 'ɋ', 'ř', 'ş',
    'ŧ', 'ŭ', 'ṽ', 'ẇ', 'ẋ', 'ẏ', 'ẑ',
];

/// `en-XA`: every ASCII letter accented, and every vowel doubled, which
/// makes English about 40% longer, as many translations are.
pub(crate) fn accented(text: &str) -> Cow<'_, str> {
    if !text.bytes().any(|byte| byte.is_ascii_alphabetic()) {
        return Cow::Borrowed(text);
    }
    let mut out = String::with_capacity(text.len() * 2);
    for ch in text.chars() {
        let accented = match ch {
            'A'..='Z' => UPPER[usize::from(ch as u8 - b'A')],
            'a'..='z' => LOWER[usize::from(ch as u8 - b'a')],
            _ => ch,
        };
        out.push(accented);
        if "aeiouyAEIOUY".contains(ch) {
            out.push(accented);
        }
    }
    Cow::Owned(out)
}

/// `en-XB`: each run of text forced right to left.
pub(crate) fn bidi(text: &str) -> Cow<'_, str> {
    if text.trim().is_empty() {
        return Cow::Borrowed(text);
    }
    Cow::Owned(format!("{RLO}{text}{PDF}"))
}

/// Whether `text` is a whole `en-XA` message.
pub fn is_marked(text: &str) -> bool {
    text.starts_with(MARKS.0) && text.ends_with(MARKS.1)
}

pub(crate) fn mark(text: String) -> String {
    format!("{}{text}{}", MARKS.0, MARKS.1)
}
