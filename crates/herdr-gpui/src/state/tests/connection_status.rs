use super::*;

#[test]
fn bells_count_only_while_connected_and_reset_with_the_connection() {
    let bell = |count| ClientEvent::Message(ServerMessage::TerminalBell { count });
    let title = || {
        ClientEvent::Message(ServerMessage::WindowTitle {
            title: Some("t".into()),
        })
    };
    let mut state = LiveState::default();
    state.apply(bell(1));
    assert_eq!(state.bells, 0, "no bells before the connection is up");

    state.status = ConnectionStatus::Connected;
    state.apply(bell(0));
    assert_eq!(state.bells, 0);
    state.apply(bell(u16::MAX));
    state.apply(bell(2));
    assert_eq!(state.bells, u16::MAX, "a burst saturates");
    assert!(!state.only_surface_changed(&state.clone()));

    state.apply(title());
    state.apply(ClientEvent::Disconnected {
        reason: "gone".into(),
    });
    assert_eq!((state.bells, state.window_title.as_deref()), (0, None));

    // A restarted daemon's first snapshot drops the old daemon's title.
    state.apply(ClientEvent::Snapshot(snapshot()));
    state.apply(title());
    state.apply(bell(1));
    let mut restarted = (*snapshot()).clone();
    restarted.boot_id = "restarted".into();
    state.apply(ClientEvent::Snapshot(Arc::new(restarted)));
    assert_eq!((state.bells, state.window_title.as_deref()), (0, None));
}

#[test]
fn rejected_dialog_command_preserves_connection_diagnostic() {
    let mut state = LiveState {
        status: ConnectionStatus::Disconnected,
        error: Some("socket closed".into()),
        dialog_response: Some(("create".into(), None)),
        ..LiveState::default()
    };
    state.apply(ClientEvent::CommandRejected {
        request_id: Some("create".into()),
        reason: herdr_client::Error::Disconnected,
    });
    assert_eq!(state.status_text(None), "Disconnected: socket closed");
    assert!(matches!(&state.dialog_response, Some((_, Some(Err(_))))));
}

#[test]
fn untracked_daemon_errors_remain_visible_as_readable_messages() {
    let mut state = LiveState {
        status: ConnectionStatus::Connected,
        ..LiveState::default()
    };
    for (payload, expected) in [
        (
            serde_json::json!({"code": "failed", "message": "Input failed"}),
            "Input failed",
        ),
        (serde_json::json!("Legacy failure"), "Legacy failure"),
        (serde_json::json!({"code": "unsupported"}), "unsupported"),
        (
            serde_json::json!({"unexpected": true}),
            "Invalid daemon error",
        ),
    ] {
        state.apply(ClientEvent::Response {
            request_id: "input".into(),
            response: serde_json::json!({"error": payload}),
        });
        assert_eq!(state.status_text(None), format!("Connected: {expected}"));
    }
}

#[test]
fn missing_installation_survives_disconnect_but_clears_on_success() {
    let mut state = LiveState::default();
    assert!(!state.missing_installation);
    state.missing_installation = true;
    state.apply(ClientEvent::Disconnected {
        reason: "Herdr not found".into(),
    });
    assert!(state.missing_installation);
    state.set_outer_focus(true);
    assert!(state.missing_installation);
    state.apply(ClientEvent::Snapshot(snapshot()));
    assert!(!state.missing_installation);
}

#[test]
fn daemon_loader_stops_on_success_or_failure() {
    let mut state = LiveState::default();
    assert_eq!(state.status, ConnectionStatus::Connecting);
    state.dirty = false;
    state.daemon_starting();
    assert!(state.dirty);
    assert_eq!(state.status, ConnectionStatus::StartingDaemon);
    assert!(!state.status.is_connected());
    assert_eq!(
        state.status_text(Some("old input error")),
        "Starting Herdr server..."
    );
    state.apply(ClientEvent::Snapshot(snapshot()));
    assert_eq!(state.status, ConnectionStatus::Connected);

    state.daemon_starting();
    state.apply(ClientEvent::Disconnected {
        reason: "startup failed".into(),
    });
    assert_eq!(state.status, ConnectionStatus::Disconnected);
    assert_eq!(state.error.as_deref(), Some("startup failed"));
}

#[test]
fn connection_status_and_error_priority_follow_lifecycle() {
    let mut state = LiveState::default();
    assert_eq!(state.status, ConnectionStatus::Connecting);
    assert!(!state.status.is_connected());
    assert_eq!(state.status_text(Some("old input error")), "Connecting...");
    state.apply(ClientEvent::Snapshot(snapshot()));
    assert!(state.status.is_connected());
    assert_eq!(
        state.status_text(Some("input error")),
        "Connected: input error"
    );
    state.apply(ClientEvent::Disconnected {
        reason: "socket closed".into(),
    });
    assert!(!state.status.is_connected());
    assert_eq!(
        state.status_text(Some("old input error")),
        "Disconnected: socket closed"
    );
    state.status = ConnectionStatus::Detached;
    state.error = None;
    assert!(!state.status.is_connected());
    assert_eq!(
        state.status_text(Some("old input error")),
        "Detached (daemon still running)"
    );
    assert!(ConnectionStatus::AwaitingSnapshot.is_connected());
    assert_eq!(
        ConnectionStatus::AwaitingSnapshot.to_string(),
        "Connected; waiting for snapshot"
    );
}
