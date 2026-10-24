//! The new worktree dialog's branch field formats what is typed into it, so a
//! character Git refuses never reaches the submit check. Only the text being
//! typed or pasted is changed, never the text around it: the caret stays right
//! after the insertion, and nothing already in the field moves under it.
//!
//! The rules are `git check-ref-format`'s, applied as each character arrives.
//! Letters keep their case and `/` keeps namespaces such as `feat/login`. An
//! ending that more typing can still make valid (a trailing `.`, `/` or
//! `.lock`) is left alone, since every name passes through one while it is
//! typed; the submit check still refuses it if it is sent as it is.

/// Characters Git refuses anywhere in a branch name besides whitespace and
/// control characters. Each becomes a dash, as a space does.
const SEPARATORS: [char; 7] = ['~', '^', ':', '?', '*', '[', '\\'];

/// `inserted` as it may go between `before` and `after`. Whitespace or one of
/// `~^:?*[\` becomes a dash, and so does `{` after `@`. Left out are other
/// control characters, whatever would become a dash at the end of pasted
/// text (a copied line often ends in a newline, a title in a `?`), a dash,
/// dot or slash that would start the name, a dot that would start a
/// component or follow another, a slash after another or after a component
/// ending in `.lock`, and a dash after another, so a typed `Fix - login`
/// reads `Fix-login`. Characters are dropped from the end of the insertion
/// until it joins `after` the same way. Empty when nothing is left.
pub(crate) fn format_typed(before: &str, inserted: &str, after: &str) -> String {
    let mut name = before.to_owned();
    // One character is a key typed, so a space there is the dash wanted next.
    // A paste's leading space still parts it from the word before.
    let inserted = match inserted.chars().nth(1) {
        Some(_) => inserted.trim_end_matches(|ch| separates(ch) || ch.is_control()),
        None => inserted,
    };
    for ch in inserted.chars() {
        if ch.is_control() && !ch.is_whitespace() {
            continue;
        }
        let last = name.chars().next_back();
        let separator = separates(ch) || (ch == '{' && last == Some('@'));
        let ch = if separator { '-' } else { ch };
        // A dash standing in for a separator also stays off a slash.
        if accepts(&name, ch) && !(separator && last == Some('/')) {
            name.push(ch);
        }
    }
    while name.len() > before.len() && !joins(&name, after) {
        name.pop();
    }
    name.split_off(before.len())
}

/// Whether `ch` always stands for a dash.
fn separates(ch: char) -> bool {
    ch.is_whitespace() || SEPARATORS.contains(&ch)
}

/// Whether `ch` may follow `name`, the text left of the caret.
fn accepts(name: &str, ch: char) -> bool {
    let last = name.chars().next_back();
    match ch {
        '-' => !matches!(last, None | Some('-')),
        '.' => !matches!(last, None | Some('.' | '/')),
        '/' => !matches!(last, None | Some('/')) && !component(name).ends_with(".lock"),
        _ => true,
    }
}

/// Whether `name` may be followed by `after`, the text right of the caret.
fn joins(name: &str, after: &str) -> bool {
    let (Some(last), Some(next)) = (name.chars().next_back(), after.chars().next()) else {
        return true;
    };
    if matches!(
        (last, next),
        ('/', '/' | '.') | ('.', '.') | ('@', '{') | ('-', '-')
    ) {
        return false;
    }
    // A component the insertion finishes before a slash cannot end in `.lock`.
    match after.split_once('/') {
        Some((rest, _)) => !format!("{}{rest}", component(name)).ends_with(".lock"),
        None => true,
    }
}

/// The last `/`-separated component of `name`.
fn component(name: &str) -> &str {
    name.rsplit('/').next().unwrap_or_default()
}

#[cfg(test)]
mod tests;
