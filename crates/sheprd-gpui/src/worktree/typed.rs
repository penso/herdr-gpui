//! The new worktree dialog's branch field formats what is typed into it, so a
//! character Git refuses never reaches the submit check. Only the text being
//! typed or pasted is changed, never the text around it: the caret stays right
//! after the insertion, and nothing already in the field moves under it.
//!
//! A deletion or cut has nothing of its own to change, so when it brings
//! together two characters Git refuses side by side, the text after the caret
//! gives way instead, and the caret stays where the deletion left it.
//!
//! The rules are `git check-ref-format`'s, applied as each character arrives.
//! Letters keep their case and `/` keeps namespaces such as `feat/login`. An
//! ending that more typing can still make valid (a trailing `.`, `/` or
//! `.lock`) is left alone, since every name passes through one while it is
//! typed; the submit check still refuses it if it is sent as it is.

use super::{LOCK, REFUSED};

/// Characters a deletion's join must copy unchanged before the text after it
/// reads as it did. Pushing a character depends only on the last one and on
/// whether the component ends in `.lock`, so this many unchanged characters,
/// or an unchanged `/` starting a fresh component, restore the old state.
const SETTLED: usize = LOCK.len();

/// An edit as the branch field applies it: `inserted` goes where the edit's
/// range was, and `after` replaces the first `replaced` bytes of the text
/// that followed the range, which a deletion can change. The rest of that
/// text is kept as it was.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Edit {
    pub inserted: String,
    pub after: String,
    pub replaced: usize,
}

/// `inserted` as it may go between `before` and `after`. Whitespace or one of
/// `~^:?*[\` becomes a dash, and so does `{` after `@`. Left out are other
/// control characters, whatever would become a dash at the end of pasted
/// text (a copied line often ends in a newline, a title in a `?`), a dash,
/// dot or slash that would start the name, a dot that would start a
/// component or follow another, a slash after another or after a component
/// ending in `.lock`, and a dash after another, so a typed `Fix - login`
/// reads `Fix-login`. Characters are dropped from the end of the insertion
/// until it joins `after` the same way. Empty when nothing is left.
fn format_typed(before: &str, inserted: &str, after: &str) -> String {
    let mut name = before.to_owned();
    // One character is a key typed, so a space there is the dash wanted next.
    // A paste's leading space still parts it from the word before.
    let inserted = match inserted.chars().nth(1) {
        Some(_) => inserted.trim_end_matches(|ch| separates(ch) || ch.is_control()),
        None => inserted,
    };
    for ch in inserted.chars() {
        push(&mut name, ch);
    }
    let next = after.chars().next();
    // What the component finished at the caret continues with up to a slash.
    let closing = after.split_once('/').map(|(rest, _)| rest);
    while name.len() > before.len() && !joins(&name, next, closing) {
        name.pop();
    }
    name.split_off(before.len())
}

/// An edit replacing what lies between `before` and `after`, as it may leave
/// the field. The insertion is `format_typed`'s, cut back so it joins `after`
/// as that text stands. A deletion inserts nothing, so `a.x.b` without its
/// `x` would read `a..b`; there the start of `after` gives way by the rules
/// typing follows: a `.`, `/` or `-` doubled collapses to one, `{` after `@`
/// becomes a dash, and a dash, dot or slash that would start the name, a dot
/// that would start a component, or a slash closing a component that ends in
/// `.lock` is left out. Only the start of `after` up to where it reads as it
/// did is rewritten, so nothing further from the caret changes, and the
/// caret goes after the insertion, so it does not move.
pub(crate) fn format_edit(before: &str, inserted: &str, after: &str) -> Edit {
    let inserted = format_typed(before, inserted, after);
    let mut name = format!("{before}{inserted}");
    let joined = name.len();
    let (mut settled, mut replaced) = (0, after.len());
    for (index, ch) in after.char_indices() {
        let len = name.len();
        push(&mut name, ch);
        let kept = name.len() == len + ch.len_utf8() && name.ends_with(ch);
        settled = if kept { settled + 1 } else { 0 };
        if kept && (ch == '/' || settled >= SETTLED) {
            replaced = index + ch.len_utf8();
            break;
        }
    }
    Edit {
        after: name.split_off(joined),
        inserted,
        replaced,
    }
}

/// Adds `ch` to the end of `name` as a key typed there would: whitespace or
/// one of `~^:?*[\` as a dash, and nothing when it would form a sequence Git
/// refuses.
fn push(name: &mut String, ch: char) {
    if ch.is_control() && !ch.is_whitespace() {
        return;
    }
    let last = name.chars().next_back();
    let separator = separates(ch) || (ch == '{' && last == Some('@'));
    let ch = if separator { '-' } else { ch };
    // A dash standing in for a separator also stays off a slash.
    if accepts(name, ch) && !(separator && last == Some('/')) {
        name.push(ch);
    }
}

/// Whether `ch` always stands for a dash.
fn separates(ch: char) -> bool {
    ch.is_whitespace() || REFUSED.contains(&ch)
}

/// Whether `ch` may follow `name`, the text left of the caret.
fn accepts(name: &str, ch: char) -> bool {
    let last = name.chars().next_back();
    match ch {
        '-' => !matches!(last, None | Some('-')),
        '.' => !matches!(last, None | Some('.' | '/')),
        '/' => !matches!(last, None | Some('/')) && !component(name).ends_with(LOCK),
        _ => true,
    }
}

/// Whether `name` may be followed by `next`, the character right of the
/// caret, with `closing` the text between the caret and the next slash.
fn joins(name: &str, next: Option<char>, closing: Option<&str>) -> bool {
    let (Some(last), Some(next)) = (name.chars().next_back(), next) else {
        return true;
    };
    if matches!(
        (last, next),
        ('/', '/' | '.') | ('.', '.') | ('@', '{') | ('-', '-')
    ) {
        return false;
    }
    // A component the insertion finishes before a slash cannot end in `.lock`.
    closing.is_none_or(|rest| !ends_in_lock(component(name), rest))
}

/// Whether `head` followed by `rest` ends in `.lock`, without joining them.
fn ends_in_lock(head: &str, rest: &str) -> bool {
    match LOCK.len().checked_sub(rest.len()) {
        None | Some(0) => rest.ends_with(LOCK),
        Some(split) => LOCK.ends_with(rest) && head.ends_with(&LOCK[..split]),
    }
}

/// The last `/`-separated component of `name`.
fn component(name: &str) -> &str {
    name.rsplit('/').next().unwrap_or_default()
}

#[cfg(test)]
mod tests;
