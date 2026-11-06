use super::*;
use sheprd_client::VersionMismatch;

#[test]
fn connected_event_records_and_sanitizes_daemon_version() {
    let mut state = LiveState::default();
    assert_eq!(state.daemon_version(), None);

    let welcome = serde_json::from_str(include_str!(
        "../../../../sheprd-protocol/tests/fixtures/endpoint-welcome-v1.json"
    ))
    .unwrap();
    state.apply(ClientEvent::Connected(welcome));
    assert_eq!(state.daemon_version(), Some("0.8.2"));

    // Dirty controls and excess length get sanitized
    let mut dirty_welcome: sheprd_client::protocol::endpoint::EndpointServerWelcome =
        serde_json::from_str(include_str!(
            "../../../../sheprd-protocol/tests/fixtures/endpoint-welcome-v1.json"
        ))
        .unwrap();
    dirty_welcome.server_version = format!(" \u{1b}[31m{}\n", "1".repeat(200));
    state.apply(ClientEvent::Connected(dirty_welcome));
    let version = state.daemon_version().unwrap();
    assert!(!version.contains('\u{1b}'));
    assert!(version.len() <= MAX_DAEMON_VERSION_CHARS);
}

#[test]
fn mismatch_event_records_daemon_version() {
    let mut state = LiveState::default();
    let mismatch = VersionMismatch::DaemonOutdated {
        server_version: Some("0.7.9".into()),
    };
    state.apply(ClientEvent::VersionMismatch(mismatch));
    assert_eq!(state.daemon_version(), Some("0.7.9"));
}

#[test]
fn disconnect_preserves_last_known_daemon_version() {
    let mut state = LiveState::default();
    let welcome = serde_json::from_str(include_str!(
        "../../../../sheprd-protocol/tests/fixtures/endpoint-welcome-v1.json"
    ))
    .unwrap();
    state.apply(ClientEvent::Connected(welcome));
    assert_eq!(state.daemon_version(), Some("0.8.2"));

    state.apply(ClientEvent::Disconnected {
        reason: "Connection reset by peer".into(),
        ssh: None,
    });
    // Still preserved after disconnection for report-issue
    assert_eq!(state.daemon_version(), Some("0.8.2"));
}
