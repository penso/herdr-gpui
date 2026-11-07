use super::*;
use sheprd_chat_protocol::{Tool, ToolContent, ToolKind, ToolStatus};

#[test]
fn an_opened_tools_source_selects_drawn_lines_and_copies_source_bytes() {
    let raw = serde_json::json!({"code": "\tlet α = 1;\nsecond", "language": "rust"}).to_string();
    let (transcript, mut prepared, keys) = prepare(vec![Entry {
        id: EntryId(7),
        kind: EntryKind::Tool(Tool {
            call_id: "call".into(),
            content: vec![ToolContent::Text { text: raw }],
            kind: ToolKind::Execute,
            locations: Vec::new(),
            parent: None,
            started_ms: None,
            status: ToolStatus::Completed,
            title: "Code".into(),
        }),
    }]);
    let region = Region::ToolPreview(0);
    assert!(
        prepared
            .text()
            .key(EntryId(7), region, Cell::default())
            .is_none(),
        "a folded card's source is not selectable"
    );
    prepared.set_expanded(&std::collections::HashSet::from([EntryId(7)]));
    prepared.update(&transcript, &keys);
    let document = prepared.text();
    let key = document.key(EntryId(7), region, Cell::default()).unwrap();
    assert_eq!(document.segment(key).unwrap().text(), "    let α = 1;");
    let mut selection = Selection::default();
    selection.select_all(document);
    assert!(
        selection
            .text(document)
            .unwrap()
            .ends_with("\tlet α = 1;\nsecond")
    );
}
