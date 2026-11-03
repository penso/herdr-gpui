use super::*;
use serde_json::json;

#[test]
fn move_serializes_herdr_s_field_names() {
    let reorder = WorkspaceReorder::Move(WorkspaceMoveParams {
        workspace_id: "w2".into(),
        insert_index: 0,
    });
    assert_eq!(reorder.method(), Method::WorkspaceMove);
    assert_eq!(
        reorder.params().unwrap(),
        json!({"workspace_id": "w2", "insert_index": 0})
    );
}

#[test]
fn move_block_keeps_an_explicit_end_anchor() {
    let reorder = WorkspaceReorder::MoveBlock(WorkspaceMoveBlockParams {
        workspace_ids: vec!["a".into(), "a1".into()],
        before_workspace_id: None,
    });
    assert_eq!(reorder.method(), Method::WorkspaceMoveBlock);
    assert_eq!(
        reorder.params().unwrap(),
        json!({"workspace_ids": ["a", "a1"], "before_workspace_id": null})
    );
    // Herdr defaults a missing anchor, so both spellings mean the end.
    let parsed: WorkspaceMoveBlockParams =
        serde_json::from_value(json!({"workspace_ids": ["a"]})).unwrap();
    assert_eq!(parsed.before_workspace_id, None);
}
