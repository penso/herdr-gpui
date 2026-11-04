use super::*;
use crate::worktree::git_accepts;

/// A composition committed as text the branch field leaves out is removed,
/// and the text either side of it joins as after a deletion.
#[test]
fn a_composition_committed_to_nothing_joins_like_a_deletion() {
    let mut input = DialogInput::branch("a.x.b".into());
    input.selection = 2..3;
    input.replace(None, "\u{3002}", true, Some(1..1));
    assert_eq!(input.text, "a.\u{3002}.b");
    input.replace(None, ".", false, None);
    assert_eq!(input.text, "a.b");
    assert_eq!(input.selection, 2..2);
    assert!(input.marked.is_none());
    assert!(git_accepts(&input.text));
}

/// A composition the platform unmarks instead of committing, as when focus
/// leaves the field mid-word, is formatted as if it had been typed, so a
/// space an IME left in it cannot reach the submit check.
#[test]
fn an_unmarked_composition_is_formatted_like_typing() {
    let mut input = DialogInput::branch("feat/".into());
    input.selection = 5..5;
    input.replace(None, "ni hao", true, Some(6..6));
    assert_eq!(input.text, "feat/ni hao");
    input.unmark();
    assert_eq!(input.text, "feat/ni-hao");
    assert_eq!(input.selection, 11..11);
    assert!(input.marked.is_none());
    assert!(git_accepts(&input.text));

    let mut input = DialogInput::branch("fix".into());
    input.selection = 3..3;
    input.replace(None, "\u{3000}ui", true, Some(3..3));
    input.unmark();
    assert_eq!(input.text, "fix-ui");
}

/// A field without a formatter keeps an unmarked composition as it stands.
#[test]
fn an_unformatted_field_keeps_an_unmarked_composition() {
    let mut input = DialogInput::new(String::new());
    input.replace(None, "ni hao", true, Some(6..6));
    input.unmark();
    assert_eq!(input.text, "ni hao");
    assert!(input.marked.is_none());
}

/// An edit near the start of a long branch only rewrites the text right
/// after it, as far as the join reaches, and leaves the rest as it was.
#[test]
fn an_edit_only_rewrites_the_text_near_it() {
    // Not text the formatter wrote, so a full rewrite would change its tail.
    let mut input = DialogInput::branch("ab-cdefgh--x".into());
    input.selection = 1..2;
    input.replace(None, "", false, None);
    assert_eq!(input.text, "a-cdefgh--x");
    input.selection = 1..2;
    input.replace(None, "z", false, None);
    assert_eq!(input.text, "azcdefgh--x");
    assert_eq!(input.selection, 2..2);
}
