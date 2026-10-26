use super::*;
use crate::{chat::discovery, state::LiveState};
use herdr_client::{ConnectTarget, protocol::ClientShellSnapshot};

const ID: &str = "0123456789abcdef0123456789abcdef";

#[test]
fn existing_sessions_keep_acp_priority_and_require_the_same_agent() {
    use crate::chat::existing::Binding;
    let mut snapshot = snapshot(&[("cli", "not-acp"), ("acp", ID)]);
    for agent in &mut snapshot.agents {
        agent.agent = Some("codex".into());
    }
    let make = |pane: &str| Binding {
        pane_id: pane.into(),
        agent: herdr_chat_protocol::Agent::Codex,
        session_id: "12345678-1234-4123-8123-123456789abc".into(),
        herdr_session: Some("test-session".into()),
        pid: 21,
        shell_pid: 20,
        started_ticks: 123,
    };
    let bindings = [make("cli"), make("acp"), make("absent")];
    let targets = discovery::with_existing(
        discovery::targets(&snapshot, Some(&Route::Local)),
        &snapshot,
        Some(&Route::Local),
        &bindings,
    );
    assert_eq!(targets.len(), 2);
    assert!(targets[0].existing.is_none());
    assert_eq!(targets[1].existing.as_ref(), Some(&bindings[0]));
    let mut replaced = targets[1].clone();
    replaced.existing.as_mut().unwrap().started_ticks += 1;
    assert_ne!(targets[1], replaced, "a reused pid needs a new observer");
    snapshot.agents[0].agent = Some("claude".into());
    assert!(
        discovery::with_existing(Vec::new(), &snapshot, Some(&Route::Local), &bindings[..1])
            .is_empty()
    );
}

fn tokens(value: &str) -> Vec<(String, String)> {
    vec![
        ("model".into(), "opus".into()),
        (herdr_chat_protocol::TOKEN.into(), value.into()),
    ]
}

#[test]
fn explicit_mirrors_are_bound_to_the_named_host_and_existing_pane() {
    use crate::config::chat::TranscriptMirror;
    let snapshot = snapshot(&[("cli", "not-acp"), ("acp", ID)]);
    let mirrors = vec![
        TranscriptMirror {
            pane: "cli".into(),
            endpoint: ID.into(),
            host: None,
        },
        TranscriptMirror {
            pane: "absent".into(),
            endpoint: ID.into(),
            host: None,
        },
        TranscriptMirror {
            pane: "acp".into(),
            endpoint: "fedcba9876543210fedcba9876543210".into(),
            host: None,
        },
    ];
    let route = Route::Local;
    let targets = discovery::with_mirrors(
        discovery::targets(&snapshot, Some(&route)),
        &snapshot,
        Some(&route),
        &mirrors,
        false,
    );
    assert_eq!(targets.len(), 2);
    assert_eq!(targets[0].pane_id, "acp");
    assert_eq!(targets[0].endpoint.as_str(), ID);
    assert_eq!(targets[1].pane_id, "cli");
    assert!(
        discovery::with_mirrors(
            Vec::new(),
            &snapshot,
            Some(&Route::Ssh("synthetic-host".into())),
            &mirrors,
            false,
        )
        .is_empty()
    );
    assert!(discovery::with_mirrors(Vec::new(), &snapshot, None, &mirrors, false).is_empty());
    assert!(
        discovery::with_mirrors(Vec::new(), &snapshot, Some(&route), &mirrors, true).is_empty(),
        "a pending/failed live lookup must not show an old static transcript"
    );
}

fn snapshot(agents: &[(&str, &str)]) -> ClientShellSnapshot {
    let mut snapshot = crate::sidebar::layout_tests::snapshot(1);
    let template = snapshot.agents[0].clone();
    snapshot.agents = agents
        .iter()
        .map(|(pane, value)| {
            let mut agent = template.clone();
            agent.pane_id = (*pane).into();
            agent.tokens = tokens(value);
            agent
        })
        .collect();
    snapshot
}

fn connected(peer: bool) -> LiveState {
    let mut live = LiveState::default();
    live.status = crate::state::ConnectionStatus::Connected;
    live.local_daemon_peer = peer;
    live
}

#[test]
fn a_valid_token_names_the_program() {
    assert_eq!(
        discovery::endpoint(&tokens(ID)).map(|id| id.to_string()),
        Some(ID.into())
    );
}

#[test]
fn an_invalid_token_names_nothing() {
    for value in [
        "",
        "0123",
        "0123456789ABCDEF0123456789ABCDEF",
        "../../../../tmp/evil.sock",
        "0123456789abcdef0123456789abcdeg",
        "/tmp/0123456789abcdef0123456789abcdef",
    ] {
        assert_eq!(discovery::endpoint(&tokens(value)), None, "{value}");
    }
    assert_eq!(discovery::endpoint(&[]), None);
}

#[test]
fn a_local_daemon_lists_each_chat_pane_once() {
    let other = "fedcba9876543210fedcba9876543210";
    let snapshot = snapshot(&[("p1", ID), ("p2", "bad"), ("p1", other), ("p3", other)]);
    let targets = discovery::targets(&snapshot, Some(&Route::Local));
    let panes: Vec<_> = targets.iter().map(|t| t.pane_id.as_str()).collect();
    assert_eq!(panes, ["p1", "p3"]);
    assert_eq!(targets[0].endpoint.as_str(), ID);
}

#[test]
fn chat_panes_are_bounded() {
    let panes: Vec<String> = (0..MAX_CHATS + 4).map(|n| format!("p{n}")).collect();
    let agents: Vec<_> = panes.iter().map(|pane| (pane.as_str(), ID)).collect();
    assert_eq!(
        discovery::targets(&snapshot(&agents), Some(&Route::Local)).len(),
        MAX_CHATS
    );
}

#[test]
fn only_an_ssh_daemon_is_reached_remotely() {
    assert!(discovery::targets(&snapshot(&[("p1", ID)]), None).is_empty());
    let live = connected(true);
    let ssh = ConnectTarget::Ssh {
        target: "host".into(),
        session: "default".into(),
    };
    assert_eq!(
        discovery::route(&ssh, &live),
        Some(Route::Ssh("host".into()))
    );
    let remote = discovery::targets(&snapshot(&[("p1", ID)]), Some(&Route::Ssh("host".into())));
    assert_eq!(remote[0].route, Route::Ssh("host".into()));
    assert_eq!(
        discovery::route(&ConnectTarget::Local, &live),
        Some(Route::Local)
    );
    for target in [
        ConnectTarget::Wsl {
            distro: "Ubuntu".into(),
            session: "default".into(),
        },
        ConnectTarget::Socket("/tmp/herdr.sock".into()),
    ] {
        assert_eq!(discovery::route(&target, &live), None, "{target:?}");
    }
}

#[test]
fn a_remote_daemon_is_not_local() {
    let live = connected(true);
    for target in [
        ConnectTarget::Ssh {
            target: "host".into(),
            session: "default".into(),
        },
        ConnectTarget::Wsl {
            distro: "Ubuntu".into(),
            session: "default".into(),
        },
        // A socket by path may be forwarded from another host.
        ConnectTarget::Socket("/tmp/herdr.sock".into()),
    ] {
        assert!(!discovery::local(&target, &live), "{target:?}");
    }
}

#[test]
fn only_an_owned_connected_local_daemon_is_local() {
    assert!(discovery::local(&ConnectTarget::Local, &connected(true)));
    assert!(discovery::local(
        &ConnectTarget::Session {
            name: "work".into(),
            development: false
        },
        &connected(true)
    ));
    // Windows has no peer check; its chat pipe's owner is checked instead.
    assert_eq!(
        discovery::local(&ConnectTarget::Local, &connected(false)),
        cfg!(windows)
    );
    assert!(!discovery::local(
        &ConnectTarget::Local,
        &LiveState::default()
    ));
}

#[test]
fn the_remote_script_prefers_herdr_bin_and_falls_back_to_path() {
    let script = remote_program_script("connect 0123456789abcdef0123456789abcdef");
    let herdr = script.find("$HOME/.herdr/bin/herdr-acp").unwrap();
    let path = script.find("$(command -v herdr-acp)").unwrap();
    assert!(herdr < path);
    assert!(script.contains("exec \"$p\" connect 0123456789abcdef0123456789abcdef"));
    assert!(script.ends_with("exit 127"));
}
