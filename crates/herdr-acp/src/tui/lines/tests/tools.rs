//! A tool call stays quiet beside the reply: a muted title without its
//! code ticks, cut after a couple of lines.

use super::*;

fn call(title: &str) -> Entry {
    Entry {
        id: EntryId(1),
        kind: EntryKind::Tool(Tool {
            call_id: "c".to_owned(),
            content: Vec::new(),
            kind: ToolKind::Execute,
            locations: Vec::new(),
            parent: None,
            status: ToolStatus::Completed,
            title: title.to_owned(),
        }),
    }
}

#[test]
fn a_command_title_is_muted_and_loses_its_ticks() {
    let lines = entry(&call("`git status --short`"), 40);
    assert_eq!(plain(&lines)[0], "✓ git status --short");
    assert_eq!(lines[0].spans[0].style.fg, Some(Color::Green));
    assert_eq!(lines[0].spans[1].style.fg, Some(Color::DarkGray));
    assert!(
        !lines[0].spans[1]
            .style
            .add_modifier
            .contains(Modifier::BOLD)
    );
}

#[test]
fn ticks_inside_a_title_are_kept() {
    let lines = plain(&entry(&call("`a` then `b`"), 40));
    assert_eq!(lines[0], "✓ `a` then `b`");
}

#[test]
fn a_long_command_is_cut_after_two_lines() {
    let title = format!("`{}`", "word ".repeat(40));
    let lines = plain(&entry(&call(&title), 30));
    assert_eq!(lines.len(), TITLE_LINES + 1, "{lines:?}");
    let last = &lines[TITLE_LINES - 1];
    assert!(last.ends_with('…'), "{last}");
    assert!(last.chars().count() <= 30, "{last}");
}
