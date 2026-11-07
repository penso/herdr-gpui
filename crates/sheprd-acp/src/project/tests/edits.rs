//! An edit reaches clients whole or as a notice, never with a side cut.

use super::*;

fn diff(old: Option<String>, new: String) -> ToolContent {
    tool_content(wire::ToolCallContent::Diff(
        wire::Diff::new("src/a.rs", new).old_text(old),
    ))
}

fn notice() -> ToolContent {
    ToolContent::Text {
        text: EDIT_NOTICE.to_owned(),
    }
}

#[test]
fn an_edit_with_either_side_past_the_limit_is_a_notice() {
    let fits = "a".repeat(MAX_ENTRY_TEXT);
    let over = "a".repeat(MAX_ENTRY_TEXT + 1);
    assert_eq!(diff(Some(fits.clone()), over.clone()), notice());
    assert_eq!(diff(Some(over.clone()), fits), notice());
    assert_eq!(diff(None, over), notice());
}

#[test]
fn an_edit_at_the_limit_keeps_both_sides_exactly() {
    let text = "a".repeat(MAX_ENTRY_TEXT);
    assert_eq!(
        diff(Some(text.clone()), text.clone()),
        ToolContent::Diff {
            new: text.clone(),
            old: Some(text),
            path: "src/a.rs".to_owned(),
        }
    );
}
