//! What leaves the agent running, before it is ever touched.

use super::*;

fn reported(agent: Option<&str>, status: AgentStatus, tokens: &[(&str, &str)]) -> ClientShellAgent {
    let mut value = serde_json::json!({
        "pane_id": "w1:p1",
        "workspace_id": "w1",
        "tab_id": "w1:t1",
        "name": null,
        "display_agent": null,
        "agent": agent,
        "title": null,
        "terminal_title": null,
        "terminal_title_stripped": null,
        "agent_status": "idle",
        "state_change_seq": 0,
        "state_labels": [],
        "focused": false,
    });
    value["tokens"] = serde_json::json!(tokens);
    let mut agent: ClientShellAgent = serde_json::from_value(value).unwrap();
    agent.agent_status = status;
    agent
}

#[test]
fn an_idle_or_finished_agent_can_move() {
    for status in [AgentStatus::Idle, AgentStatus::Done] {
        assert_eq!(
            check(&reported(Some("claude"), status, &[])).unwrap(),
            Agent::Claude
        );
    }
}

#[test]
fn an_agent_that_is_not_known_idle_stays() {
    for status in [
        AgentStatus::Working,
        AgentStatus::Blocked,
        AgentStatus::Unknown,
    ] {
        assert!(matches!(
            check(&reported(Some("codex"), status, &[])),
            Err(Error::Busy(Agent::Codex))
        ));
    }
}

#[test]
fn every_agent_is_decided() {
    let moving: Vec<_> = Agent::ALL
        .iter()
        .copied()
        .filter(|agent| convertible(*agent))
        .collect();
    for agent in [Agent::Claude, Agent::Codex, Agent::Copilot, Agent::Opencode] {
        assert!(moving.contains(&agent));
    }
    for agent in [Agent::Gemini, Agent::Pi] {
        assert!(matches!(
            check(&reported(Some(agent.name()), AgentStatus::Idle, &[])),
            Err(Error::Unsupported(found)) if found == agent
        ));
    }
}

#[test]
fn a_pane_without_a_known_agent_is_refused() {
    assert!(matches!(
        check(&reported(None, AgentStatus::Idle, &[])),
        Err(Error::NoAgent)
    ));
    assert!(matches!(
        check(&reported(Some("cursor"), AgentStatus::Idle, &[])),
        Err(Error::Unknown(name)) if name == "cursor"
    ));
}

#[test]
fn a_session_must_come_from_the_agents_own_integration() {
    let claimed = pane(serde_json::json!({"agent_session": {
        "agent": "claude", "kind": "id", "source": "someone:else", "value": SESSION,
    }}));
    assert!(matches!(
        session(&claimed, Agent::Claude),
        Err(Error::NoSession(Agent::Claude))
    ));
    let missing = pane(serde_json::json!({"agent_session": null}));
    assert!(matches!(
        session(&missing, Agent::Claude),
        Err(Error::NoSession(Agent::Claude))
    ));
}

#[test]
fn the_hosts_fresh_answer_must_show_a_finished_turn() {
    for status in ["idle", "done"] {
        let found = pane(serde_json::json!({"agent_status": status}));
        assert!(settled(&found, Agent::Claude).is_ok(), "{status}");
    }
    let found = [
        pane(serde_json::json!({"agent_status": "working"})),
        pane(serde_json::json!({"agent_status": "blocked"})),
        pane(serde_json::json!({"agent_status": "unknown"})),
        pane(serde_json::json!({"agent_status": "some_future_status"})),
        pane(serde_json::json!({"agent_status": null})),
        pane(serde_json::json!({})),
    ];
    for pane in &found {
        assert!(
            matches!(
                settled(pane, Agent::Claude),
                Err(Error::Busy(Agent::Claude))
            ),
            "{:?}",
            pane.status
        );
    }
}

#[test]
fn another_agent_in_the_pane_is_not_stopped() {
    let found = pane(serde_json::json!({"agent": "codex", "agent_status": "idle"}));
    assert!(matches!(settled(&found, Agent::Claude), Err(Error::Moved)));
}

#[test]
fn a_missing_session_says_how_to_get_one() {
    let message = Error::NoSession(Agent::Claude).to_string();
    assert!(
        message.contains("herdr integration install claude"),
        "{message}"
    );
}

#[test]
fn a_session_that_could_read_as_an_option_is_refused() {
    let flag = pane(serde_json::json!({"agent_session": {
        "agent": "claude", "kind": "id", "source": "herdr:claude", "value": "--evil",
    }}));
    assert!(session(&flag, Agent::Claude).is_err());
}

#[test]
fn a_chat_program_in_front_is_not_the_agent() {
    let running = info(
        &["/Applications/Herdr.app/sheprd-acp", "claude"],
        Some("/work"),
    );
    assert!(matches!(
        resume(&pane(serde_json::json!({})), &running, Agent::Claude),
        Err(Error::NotRunning(Agent::Claude))
    ));
}
