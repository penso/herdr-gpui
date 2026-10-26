//! The names a sent prompt gives the pane and its tab: one line, cut at a
//! word where one is near, so a tab strip or a phone's row shows it whole.

/// The most characters of the pane's title, which lists show cut further.
const TITLE: usize = 80;
/// The most characters of a tab label, the agent's name included.
const LABEL: usize = 40;
const ELLIPSIS: char = '\u{2026}';

/// `prompt` as the pane's title: its words on one line, at most [`TITLE`]
/// characters; `None` when it has none.
pub(super) fn title(prompt: &str) -> Option<String> {
    let line = one_line(prompt);
    (!line.is_empty()).then(|| shorten(&line, TITLE))
}

/// The tab label for `title` in a chat with `agent`, at most [`LABEL`]
/// characters.
pub(super) fn label(agent: &str, title: &str) -> String {
    shorten(&format!("{agent}: {title}"), LABEL)
}

/// Whether the tab's `current` label is one a chat may replace: the agent's
/// name it opened with, the label this chat gave it last, or Herdr's own
/// numbered name. Any other is the user's, and stays.
pub(super) fn replaceable(current: &str, agent: &str, given: Option<&str>) -> bool {
    current == agent
        || given == Some(current)
        || (!current.is_empty() && current.bytes().all(|byte| byte.is_ascii_digit()))
}

/// The words of `text` joined by single spaces, without control characters,
/// so a pasted escape cannot reach a terminal that shows the label.
fn one_line(text: &str) -> String {
    text.split_whitespace()
        .map(|word| word.chars().filter(|c| !c.is_control()).collect::<String>())
        .filter(|word| !word.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

/// `text` cut to `max` characters with an ellipsis, at the last space when
/// it leaves most of the room used.
fn shorten(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_owned();
    }
    let kept: String = text.chars().take(max.saturating_sub(1)).collect();
    let cut = match kept.rfind(' ') {
        Some(space) if kept[..space].chars().count() >= max / 2 => &kept[..space],
        _ => kept.as_str(),
    };
    let mut short = cut.trim_end().to_owned();
    short.push(ELLIPSIS);
    short
}
