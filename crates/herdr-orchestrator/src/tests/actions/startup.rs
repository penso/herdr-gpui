use super::*;

#[test]
fn busy_new_shell_keeps_its_worktree_and_sends_exactly_one_prompt_when_ready() {
    let dir = tempfile::tempdir().unwrap();
    let site = site(dir.path(), "");
    std::fs::write(dir.path().join("home/busy"), "2\n").unwrap();
    assert_eq!(
        perform(
            &site,
            &Action::Dispatch(Box::new(request())),
            &AtomicBool::new(false),
        )
        .unwrap(),
        "Agent started"
    );
    let calls = calls(dir.path());
    assert_eq!(
        calls
            .iter()
            .filter(|call| call.starts_with("agent prompt "))
            .count(),
        1,
        "{calls:?}"
    );
    assert!(
        !calls
            .iter()
            .any(|call| call.starts_with("worktree remove "))
    );
    assert!(calls[1].starts_with("worktree create "));
    assert_eq!(calls[2], calls[4]);
    assert_eq!(calls[2], calls[6]);
    assert!(calls[2].starts_with("agent start herdr-gpui-"));
    assert_eq!(calls[3], "pane get p9");
    assert_eq!(calls[5], "pane get p9");
    assert!(calls.last().unwrap().starts_with("agent prompt p9 "));
    let store = Store::open(&site.database).unwrap();
    let run = store.runs().unwrap().remove(0);
    assert_eq!(run.state, RunState::Running);
    assert_eq!(run.workspace.unwrap().id, "w9");
    assert_eq!(store.sessions().unwrap()[0].pane_id, "p9");
}

#[test]
fn a_missing_or_replaced_terminal_cannot_receive_a_retried_launch() {
    for pane in [
        serde_json::json!({"pane_id": "p9", "terminal_id": "replacement"}),
        serde_json::json!({"pane_id": "p9"}),
        serde_json::json!({"pane_id": "other", "terminal_id": "term9"}),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let site = site(dir.path(), "");
        std::fs::write(dir.path().join("home/busy"), "1\n").unwrap();
        std::fs::write(
            dir.path().join("home/pane"),
            serde_json::json!({"result": {"pane": pane}}).to_string(),
        )
        .unwrap();
        let error = perform(
            &site,
            &Action::Dispatch(Box::new(request())),
            &AtomicBool::new(false),
        )
        .unwrap_err();
        assert!(matches!(
            error,
            Error::Script {
                operation: "Starting the agent",
                ..
            }
        ));
        let calls = calls(dir.path());
        assert_eq!(calls.len(), 5, "{calls:?}");
        assert!(calls[2].starts_with("agent start "));
        assert_eq!(calls[3], "pane get p9");
        assert_eq!(calls[4], "worktree remove --workspace w9 --force");
        let store = Store::open(&site.database).unwrap();
        let run = store.runs().unwrap().remove(0);
        assert_eq!(run.state, RunState::Failed);
        assert!(run.workspace.is_none());
        assert!(store.sessions().unwrap().is_empty());
    }
}
