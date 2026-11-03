//! `--resume`: continuing a session, never replacing it with a new one.

use super::*;

#[test]
fn resume_refuses_a_new_session_when_the_agent_cannot_continue() {
    let mut harness = harness_with(Config {
        load: Some("old".to_owned()),
        require_load: true,
        ..config()
    });
    harness.session.start();
    let id = harness.last_request("initialize")["id"].as_i64().unwrap();
    harness.respond(id, json!({"protocolVersion": 1, "agentCapabilities": {}}));
    let sent = harness.sent();
    assert!(
        sent.iter().all(
            |message| message["method"] != "session/new" && message["method"] != "session/load"
        ),
        "{sent:?}"
    );
    assert!(matches!(harness.transcript().status, Status::Failed { .. }));
    assert_eq!(harness.notices()[0].0, NoticeLevel::Error);
    assert!(harness.opened.borrow().is_empty());
}

#[test]
fn resume_continues_a_session_the_agent_can_load() {
    let mut harness = harness_with(Config {
        load: Some("old".to_owned()),
        require_load: true,
        ..config()
    });
    harness.session.start();
    let id = harness.last_request("initialize")["id"].as_i64().unwrap();
    harness.respond(
        id,
        json!({"protocolVersion": 1, "agentCapabilities": {"loadSession": true}}),
    );
    let load = harness.last_request("session/load");
    harness.respond(load["id"].as_i64().unwrap(), json!({}));
    assert_eq!(*harness.opened.borrow(), ["old"]);
}

#[test]
fn a_new_session_reports_its_id_once_open() {
    let mut harness = harness_with(config());
    harness.session.start();
    let id = harness.last_request("initialize")["id"].as_i64().unwrap();
    harness.respond(id, json!({"protocolVersion": 1, "agentCapabilities": {}}));
    let new = harness.last_request("session/new");
    assert!(harness.opened.borrow().is_empty());
    harness.respond(new["id"].as_i64().unwrap(), json!({"sessionId": "fresh"}));
    assert_eq!(*harness.opened.borrow(), ["fresh"]);
}
