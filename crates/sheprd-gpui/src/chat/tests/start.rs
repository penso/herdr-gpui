//! Starting a new chat: finding `herdr-acp`, the tab it runs in, the fixed
//! line typed there, and the ways a start is refused.

use super::*;
use crate::{
    NavigationTarget, SheprdWindow,
    chat::{
        Program,
        start::{self, PROGRAM_ENV, command_line, locate_in, tab_params},
    },
    sidebar::layout_tests::fixture_window,
    window::MockPeer,
    worktree_scripts::tests::connect,
};
use serde_json::json;
use sheprd_chat_protocol::Agent;
use sheprd_client::protocol::{ClientMessage as DaemonMessage, ClientPaneInputEvent};
use std::path::Path;

fn program(dir: &Path) -> PathBuf {
    let path = dir.join(if cfg!(windows) {
        "herdr-acp.exe"
    } else {
        "herdr-acp"
    });
    std::fs::write(&path, "").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    path
}

#[test]
fn the_program_beside_the_app_wins_over_path() {
    let app = tempfile::tempdir().unwrap();
    let elsewhere = tempfile::tempdir().unwrap();
    let on_path = program(elsewhere.path());
    let exe = app.path().join("Herdr");
    let path = std::env::join_paths([elsewhere.path()]).unwrap();
    assert_eq!(locate_in(Some(&exe), Some(&path)), Some(on_path));
    let beside = program(app.path());
    assert_eq!(locate_in(Some(&exe), Some(&path)), Some(beside));
    assert_eq!(locate_in(None, None), None);
}

#[cfg(unix)]
#[test]
fn a_linked_app_finds_the_program_beside_its_real_file() {
    let real = tempfile::tempdir().unwrap();
    let bin = tempfile::tempdir().unwrap();
    let beside = program(real.path());
    std::fs::write(real.path().join("herdr-gpui"), "").unwrap();
    let link = bin.path().join("herdr-gpui");
    std::os::unix::fs::symlink(real.path().join("herdr-gpui"), &link).unwrap();
    assert_eq!(
        locate_in(Some(&link), None),
        Some(std::fs::canonicalize(beside).unwrap())
    );
}

#[cfg(unix)]
#[test]
fn a_file_that_cannot_run_is_not_the_program() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let path = program(dir.path());
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
    assert_eq!(locate_in(Some(&dir.path().join("Herdr")), None), None);
}

#[test]
fn the_typed_line_names_only_the_agent() {
    for agent in Agent::ALL {
        let line = command_line(*agent);
        assert!(line.contains(PROGRAM_ENV));
        assert!(line.ends_with(&format!("{agent}'")) || line.ends_with(agent.name()));
        assert!(!line.contains('/'), "{line}");
    }
    if cfg!(unix) {
        assert_eq!(
            command_line(Agent::Claude),
            r#" sh -c 'exec "$HERDR_ACP" claude'"#
        );
    } else {
        assert_eq!(command_line(Agent::Claude), "& $env:HERDR_ACP claude");
    }
}

#[test]
fn the_tab_carries_the_program_in_its_environment() {
    let none = std::collections::BTreeMap::new();
    let params = tab_params(
        "w1",
        Some("/repo"),
        Agent::Codex,
        Some(Path::new("/opt/herdr-acp")),
        &none,
    );
    assert_eq!(
        params,
        json!({"workspace_id": "w1", "label": "Codex", "focus": true, "cwd": "/repo",
            "env": {"HERDR_ACP": "/opt/herdr-acp"}})
    );
    let params = tab_params(
        "w1",
        None,
        Agent::Codex,
        Some(Path::new("/opt/herdr-acp")),
        &none,
    );
    assert!(params.get("cwd").is_none());
    // A remote chat's tab carries no local path.
    assert!(
        tab_params("w1", None, Agent::Codex, None, &none)
            .get("env")
            .is_none()
    );
}

#[test]
fn the_chat_environment_goes_with_every_chat_tab() {
    let extra = std::collections::BTreeMap::from([
        ("GH_HOST".to_owned(), "mycompany.ghe.com".to_owned()),
        (
            "ANTHROPIC_BASE_URL".to_owned(),
            "https://llm.example.com".to_owned(),
        ),
    ]);
    let local = tab_params(
        "w1",
        None,
        Agent::Claude,
        Some(Path::new("/opt/herdr-acp")),
        &extra,
    );
    assert_eq!(
        local["env"],
        json!({"GH_HOST": "mycompany.ghe.com", "ANTHROPIC_BASE_URL": "https://llm.example.com",
            "HERDR_ACP": "/opt/herdr-acp"})
    );
    // On an SSH host too, without the local program.
    let remote = tab_params("w1", None, Agent::Copilot, None, &extra);
    assert_eq!(remote["env"]["GH_HOST"], "mycompany.ghe.com");
    assert!(remote["env"].get("HERDR_ACP").is_none());
}

#[test]
fn a_remote_line_finds_the_hosts_own_program() {
    let line = start::remote_command_line(Agent::Opencode);
    assert!(line.starts_with(" sh -c '"));
    assert!(line.ends_with("exit 127'"));
    assert!(line.contains("$HOME/.herdr/bin/herdr-acp"));
    assert!(line.contains("exec \"$p\" opencode"));
    // Wrapped in single quotes, so the script must hold none of its own.
    assert_eq!(line.matches('\'').count(), 2);
}

/// A window on a local daemon this user owns, with `program` found.
fn local_window(view: &mut SheprdWindow, peer: &MockPeer, cx: &mut gpui::Context<SheprdWindow>) {
    connect(view, peer, cx);
    view.config.features.acp_chats = true;
    view.endpoints[0].connection.target = sheprd_client::ConnectTarget::Local;
    view.live.local_daemon_peer = true;
    view.endpoints[0].live = view.live.clone();
    view.chats.program = Program::Found(PathBuf::from("/opt/herdr-acp"));
}

#[gpui::test]
fn a_new_chat_opens_a_tab_and_types_the_fixed_line(cx: &mut gpui::TestAppContext) {
    let mut peer = MockPeer::advertising(&["tab.create"]);
    let (view, cx) = cx.add_window_view(fixture_window);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            local_window(view, &peer, cx);
            assert!(view.can_start_chat());
            view.start_chat(Agent::Claude, None).unwrap();
            // One tab opens at a time, a script's included.
            assert!(view.script_slot_busy());
            assert!(matches!(
                view.start_chat(Agent::Codex, None),
                Err(start::Error::SlotBusy)
            ));
        })
    });
    let tab = peer.request();
    assert_eq!(tab["method"], "tab.create");
    assert_eq!(
        tab["params"],
        json!({"workspace_id": "w1", "label": "Claude", "focus": true, "cwd": "/repo",
            "env": {"HERDR_ACP": "/opt/herdr-acp"}})
    );
    let created = json!({"result":{"type":"tab_created",
        "tab":{"tab_id":"w1:t9"},"root_pane":{"pane_id":"w1:p9"}}});
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            let id = tab["id"].as_str().unwrap().to_owned();
            view.live.script_response = Some((id, Some(Ok(created))));
            view.poll_chat_start(cx);
            assert!(!view.chat_starting());
            assert!(view.flash.is_none(), "{:?}", view.flash);
            let fresh = crate::chat::last_mode::Fresh::Pane("w1:p9".into());
            assert!(view.chats.awaits(&fresh));
            // Shown at once on a ready surface, or once it is ready.
            assert!(
                view.pending_navigation.is_none()
                    || view.pending_navigation == Some(NavigationTarget::Tab("w1:t9".into()))
            );
        })
    });
    let typed = std::iter::repeat_with(|| peer.receive())
        .take(8)
        .find(|message| matches!(message, DaemonMessage::ClientShellPaneInput { .. }));
    assert_eq!(
        typed.unwrap(),
        DaemonMessage::ClientShellPaneInput {
            pane_id: "w1:p9".into(),
            events: vec![
                ClientPaneInputEvent::TextCommit(command_line(Agent::Claude)),
                crate::menu::enter_key(),
            ],
        }
    );
}

#[gpui::test]
fn a_chat_does_not_start_without_the_program_or_a_reachable_host(cx: &mut gpui::TestAppContext) {
    let peer = MockPeer::advertising(&["tab.create"]);
    let (view, cx) = cx.add_window_view(fixture_window);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            local_window(view, &peer, cx);
            view.chats.program = Program::Missing;
            assert!(!view.can_start_chat());
            assert!(matches!(
                view.start_chat(Agent::Claude, None),
                Err(start::Error::ProgramMissing)
            ));
            view.chats.program = Program::Looking;
            assert!(matches!(
                view.start_chat(Agent::Claude, None),
                Err(start::Error::ProgramUnknown)
            ));
            view.chats.program = Program::Found(PathBuf::from("/opt/herdr-acp"));
            // An SSH host starts its own program, found there.
            view.endpoints[0].connection.target = sheprd_client::ConnectTarget::Ssh {
                target: "host".into(),
                session: "default".into(),
            };
            assert!(view.can_start_chat());
            // A socket by path may be another host's, with no way to reach it.
            view.endpoints[0].connection.target =
                sheprd_client::ConnectTarget::Socket(PathBuf::from("/tmp/herdr.sock"));
            assert!(!view.can_start_chat());
            assert!(matches!(
                view.start_chat(Agent::Claude, None),
                Err(start::Error::NotLocal)
            ));
            assert!(!view.chat_starting());
        })
    });
}

#[gpui::test]
fn a_start_from_a_replaced_connection_types_nothing(cx: &mut gpui::TestAppContext) {
    let peer = MockPeer::advertising(&["tab.create"]);
    let (view, cx) = cx.add_window_view(fixture_window);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            local_window(view, &peer, cx);
            view.start_chat(Agent::Gemini, None).unwrap();
            view.endpoints[0].generation += 1;
            view.poll_chat_start(cx);
            assert!(!view.chat_starting());
            assert!(view.flash.is_some(), "the user is told it did not start");
        })
    });
}

#[gpui::test]
fn an_agent_the_host_lacks_does_not_start(cx: &mut gpui::TestAppContext) {
    let peer = MockPeer::advertising(&["tab.create"]);
    let (view, cx) = cx.add_window_view(fixture_window);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            local_window(view, &peer, cx);
            let id = view.endpoints[0].id.clone();
            view.chats.agents.record(&id, vec![Agent::Opencode]);
            assert!(matches!(
                view.start_chat(Agent::Claude, None),
                Err(start::Error::AgentMissing(Agent::Claude))
            ));
            assert!(!view.chat_starting());
        })
    });
}
