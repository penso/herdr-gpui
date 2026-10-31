use super::*;
use herdr_chat_protocol::{Tool, ToolContent, ToolKind, ToolStatus};

#[test]
fn tool_preview_copy_includes_only_visible_source_and_preserves_tabs() {
    let source = "\tlet α = 1;\nsecond\nthird\nfourth\nfifth\nsixth";
    let raw =
        serde_json::json!({"code": source, "language": "rust", "hidden": "metadata"}).to_string();
    let (transcript, mut prepared, keys) = prepare(vec![Entry {
        id: EntryId(7),
        kind: EntryKind::Tool(Tool {
            parent: None,
            call_id: "synthetic-call".into(),
            content: vec![ToolContent::Text { text: raw }],
            kind: ToolKind::Execute,
            locations: Vec::new(),
            status: ToolStatus::InProgress,
            title: "Synthetic code".into(),
        }),
    }]);
    let region = Region::ToolPreview(0);
    assert!(
        prepared
            .text()
            .key(EntryId(7), region, Cell::default())
            .is_none()
    );
    let mut selection = Selection::default();
    selection.select_all(prepared.text());
    assert_eq!(
        selection.text(prepared.text()).as_deref(),
        Some("Synthetic code")
    );
    prepared.set_expanded(&std::collections::HashSet::from([EntryId(7)]));
    prepared.update(&transcript, &keys);
    let document = prepared.text();
    let key = document.key(EntryId(7), region, Cell::default()).unwrap();
    assert_eq!(document.segment(key).unwrap().text(), "    let α = 1;");
    assert!(document.key(EntryId(7), region, Cell::row(5)).is_some());
    selection.press(document, Caret { key, offset: 0 }, 1, false);
    let last = document.key(EntryId(7), region, Cell::row(3)).unwrap();
    selection.drag(
        document,
        Caret {
            key: last,
            offset: 6,
        },
    );
    assert_eq!(
        selection.text(document).as_deref(),
        Some("\tlet α = 1;\nsecond\nthird\nfourth")
    );
    selection.select_all(prepared.text());
    let copied = selection.text(prepared.text()).unwrap();
    assert!(copied.contains(source));
    assert!(!copied.contains("metadata"));
    prepared.set_expanded(&std::collections::HashSet::new());
    prepared.update(&transcript, &keys);
    assert!(selection.validate(prepared.text()));
    assert!(selection.text(prepared.text()).is_none());
    assert!(
        prepared
            .text()
            .key(EntryId(7), region, Cell::default())
            .is_none()
    );
    selection.select_all(prepared.text());
    assert_eq!(
        selection.text(prepared.text()).as_deref(),
        Some("Synthetic code")
    );
}

#[test]
fn diff_selection_matches_both_visible_sides_and_keeps_the_patch_markers() {
    let (transcript, mut prepared, keys) = prepare(vec![Entry {
        id: EntryId(8),
        kind: EntryKind::Tool(Tool {
            parent: None,
            call_id: "synthetic-edit".into(),
            content: vec![ToolContent::Diff {
                path: "example.rs".into(),
                old: Some("removed α".into()),
                new: "added 🦀".into(),
            }],
            kind: ToolKind::Edit,
            locations: Vec::new(),
            status: ToolStatus::Completed,
            title: "Update example".into(),
        }),
    }]);
    let mut selection = Selection::default();
    selection.select_all(prepared.text());
    let copied = selection.text(prepared.text()).unwrap();
    assert!(!copied.contains("removed α"));
    assert!(!copied.contains("added 🦀"));
    prepared.set_expanded(&std::collections::HashSet::from([EntryId(8)]));
    prepared.update(&transcript, &keys);
    selection.select_all(prepared.text());
    let copied = selection.text(prepared.text()).unwrap();
    assert!(copied.contains("-removed α"));
    assert!(copied.contains("+added 🦀"));
    assert!(copied.find("removed α").unwrap() < copied.find("added 🦀").unwrap());
    assert!(
        prepared
            .text()
            .key(EntryId(8), Region::ToolBefore(0), Cell::default())
            .is_none()
    );
}
