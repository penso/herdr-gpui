use super::*;

/// The merged worktree cleanup and an open dialog each wait on their own
/// request, so neither can take, or overwrite, the other's answer.
#[test]
fn cleanup_and_dialog_answers_land_in_their_own_slots() {
    let mut state = LiveState {
        status: ConnectionStatus::Connected,
        dialog_response: Some(("list".into(), None)),
        cleanup_response: Some(("remove".into(), None)),
        ..LiveState::default()
    };
    let refusal = serde_json::json!({"error": {
        "code": "dirty_worktree_requires_force", "message": "modified or untracked files"
    }});
    state.apply(ClientEvent::Response {
        request_id: "remove".into(),
        response: refusal.clone(),
    });
    assert!(matches!(&state.cleanup_response,
        Some((id, Some(Ok(value)))) if id == "remove" && value == &refusal));
    assert!(matches!(&state.dialog_response, Some((id, None)) if id == "list"));
    // The refusal is the cleanup's to report, not a connection error.
    assert_eq!(state.error, None);

    state.apply(ClientEvent::Response {
        request_id: "list".into(),
        response: serde_json::json!({"result": {"type": "worktree_list"}}),
    });
    assert!(matches!(&state.dialog_response, Some((id, Some(Ok(_)))) if id == "list"));
    assert!(matches!(&state.cleanup_response, Some((id, Some(Ok(_)))) if id == "remove"));

    state.cleanup_response = Some(("again".into(), None));
    state.apply(ClientEvent::CommandRejected {
        request_id: Some("again".into()),
        reason: sheprd_client::Error::CommandBoot,
    });
    assert!(matches!(&state.cleanup_response, Some((id, Some(Err(_)))) if id == "again"));
}
