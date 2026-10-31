#[test]
fn malformed_worktree_list_retains_parser_source() {
    use std::error::Error;
    let result = super::listing(&serde_json::json!({"result": {
        "type": "worktree_list", "source": {"repo_key":"key", "repo_name":"repo"},
        "worktrees": [{"path": "/checkout", "label": 42}]
    }}));
    let Err(error) = result else {
        panic!("invalid entry accepted")
    };
    assert!(matches!(error, crate::Error::WorktreeListDecode(_)));
    assert!(
        error
            .source()
            .is_some_and(|source| source.is::<serde_json::Error>())
    );
    assert_eq!(
        error.to_string(),
        "Malformed worktree list. Dismiss and reopen the menu."
    );
}
