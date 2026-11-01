use super::*;

/// A creation whose landing waits on it keeps its answer in its own slot, so
/// a newer dialog's request, sent after the first dialog closed, cannot take
/// it, and a rejection reaches the landing too.
#[test]
fn a_landing_keeps_its_creation_answer_when_a_newer_dialog_asks() {
    let mut state = LiveState {
        status: ConnectionStatus::Connected,
        // The closed dialog's creation, then a newer dialog's listing.
        dialog_response: Some(("list".into(), None)),
        landing_response: Some(("create".into(), None)),
        ..LiveState::default()
    };
    let created = serde_json::json!({"result": {"type": "worktree_created"}});
    state.apply(ClientEvent::Response {
        request_id: "create".into(),
        response: created.clone(),
    });
    assert!(matches!(&state.landing_response,
        Some((id, Some(Ok(value)))) if id == "create" && value == &created));
    assert!(matches!(&state.dialog_response, Some((id, None)) if id == "list"));

    state.landing_response = Some(("again".into(), None));
    state.apply(ClientEvent::CommandRejected {
        request_id: Some("again".into()),
        reason: herdr_client::Error::CommandBoot,
    });
    assert!(matches!(&state.landing_response, Some((id, Some(Err(_)))) if id == "again"));
    // The rejection is the landing's to report, not a connection error.
    assert_eq!(state.error, None);
}
