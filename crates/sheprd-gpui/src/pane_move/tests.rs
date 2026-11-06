#![allow(clippy::unwrap_used)]

use super::*;
use sheprd_client::ConnectTarget;

mod tab_moves;

fn snapshot() -> ClientShellSnapshot {
    let mut snapshot: ClientShellSnapshot = serde_json::from_str(include_str!(
        "../../../sheprd-protocol/tests/fixtures/endpoint-snapshot-v1.json"
    ))
    .unwrap();
    let mut second = snapshot.tabs[0].clone();
    second.tab_id = "w1:t2".into();
    second.number = 2;
    second.label = "logs".into();
    let mut workspace = snapshot.workspaces[0].clone();
    workspace.workspace_id = "w2".into();
    workspace.label = "other".into();
    let mut remote = snapshot.tabs[0].clone();
    remote.tab_id = "w2:t1".into();
    remote.workspace_id = "w2".into();
    remote.label = "build".into();
    // Out of number order: the choices follow the tab numbers.
    snapshot.tabs.insert(0, second);
    snapshot.workspaces.push(workspace);
    snapshot.tabs.push(remote);
    snapshot
}

#[test]
fn choices_skip_the_own_tab_and_offer_new_tabs_elsewhere() {
    let snapshot = snapshot();
    let choices = choices(&snapshot, "w1:t1");
    assert_eq!(
        choices,
        [
            Choice {
                destination: Destination::Tab {
                    tab_id: "w1:t2".into()
                },
                workspace: Some("repo".into()),
                place: "logs".into(),
            },
            Choice {
                destination: Destination::Tab {
                    tab_id: "w2:t1".into()
                },
                workspace: Some("other".into()),
                place: "build".into(),
            },
            Choice {
                destination: Destination::NewTab {
                    workspace_id: "w2".into()
                },
                workspace: Some("other".into()),
                place: "New Tab".into(),
            },
        ]
    );
    assert!(choices.iter().all(|c| c.destination.exists(&snapshot)));
    let mut gone = snapshot.clone();
    gone.tabs.retain(|tab| tab.workspace_id != "w2");
    gone.workspaces.retain(|w| w.workspace_id != "w2");
    assert!(!choices[1].destination.exists(&gone));
    assert!(!choices[2].destination.exists(&gone));
    assert!(Destination::NewWorkspace.exists(&gone));
    // A lone tab in a lone workspace has nowhere to go but new places.
    let lone: ClientShellSnapshot = serde_json::from_str(include_str!(
        "../../../sheprd-protocol/tests/fixtures/endpoint-snapshot-v1.json"
    ))
    .unwrap();
    assert!(super::choices(&lone, "w1:t1").is_empty());
}

#[test]
fn arguments_name_every_target_explicitly() {
    let tab = Request::new(
        "w1:p1".into(),
        Destination::Tab {
            tab_id: "w2:t1".into(),
        },
    )
    .unwrap();
    assert_eq!(
        tab.args(),
        [
            "pane", "move", "w1:p1", "--tab", "w2:t1", "--split", "right", "--focus"
        ]
    );
    let new_tab = Request::new(
        "w1:p1".into(),
        Destination::NewTab {
            workspace_id: "w1".into(),
        },
    )
    .unwrap();
    assert_eq!(
        new_tab.args(),
        [
            "pane",
            "move",
            "w1:p1",
            "--new-tab",
            "--workspace",
            "w1",
            "--focus"
        ]
    );
    let workspace = Request::new("w1:p1".into(), Destination::NewWorkspace).unwrap();
    assert_eq!(
        workspace.args(),
        ["pane", "move", "w1:p1", "--new-workspace", "--focus"]
    );
}

#[test]
fn ids_the_cli_would_misread_are_refused() {
    for id in ["", "--new-workspace", "w1\np1"] {
        assert!(matches!(
            Request::new(id.into(), Destination::NewWorkspace),
            Err(Error::InvalidId(rejected)) if rejected == id
        ));
        assert!(matches!(
            Request::new("w1:p1".into(), Destination::Tab { tab_id: id.into() }),
            Err(Error::InvalidId(_))
        ));
        assert!(matches!(
            Request::new(
                "w1:p1".into(),
                Destination::NewTab {
                    workspace_id: id.into()
                }
            ),
            Err(Error::InvalidId(_))
        ));
    }
}

#[test]
fn outcomes_follow_the_envelope() {
    assert!(
        outcome(br#"{"id":"cli:pane:move","result":{"type":"pane_move","move_result":{"changed":true,"previous_pane_id":"w1:p1"}}}"#)
            .is_ok()
    );
    for (reason, expected) in [
        ("\"same_tab\"", Unchanged::SameTab),
        ("\"zoomed_tab\"", Unchanged::ZoomedTab),
        ("\"from_the_future\"", Unchanged::Other),
        ("null", Unchanged::Other),
    ] {
        let json = format!(
            r#"{{"result":{{"type":"pane_move","move_result":{{"changed":false,"reason":{reason}}}}}}}"#
        );
        assert!(matches!(
            outcome(json.as_bytes()),
            Err(Error::Unchanged(found)) if found == expected
        ));
    }
    assert!(matches!(outcome(b"not json"), Err(Error::Decode(_))));
    assert_eq!(
        Error::Unchanged(Unchanged::ZoomedTab).to_string(),
        "A zoomed tab cannot give or take panes. Unzoom it and try again."
    );
}

#[test]
#[cfg(unix)]
fn script_failures_keep_herdr_messages_and_sources() {
    use std::os::unix::process::ExitStatusExt;
    let exit = |code: i32, stderr: &str| sheprd_client::Error::ScriptExit {
        status: std::process::ExitStatus::from_raw(code << 8),
        stderr: stderr.into(),
    };
    let refused = failure(exit(
        1,
        "warning\n{\"id\":\"cli:pane:move\",\"error\":{\"code\":\"tab_not_found\",\"message\":\"tab w9:t1 not found\"}}\n",
    ));
    assert!(matches!(
        &refused,
        Error::Refused { code, message } if code == "tab_not_found" && message == "tab w9:t1 not found"
    ));
    assert_eq!(
        refused.to_string(),
        "Could not move the pane: tab w9:t1 not found"
    );
    let missing = failure(exit(127, "sh: herdr: not found"));
    assert!(matches!(missing, Error::MissingCli(_)));
    assert!(std::error::Error::source(&missing).is_some());
    let usage = failure(exit(2, "usage: herdr pane move"));
    assert!(matches!(
        usage,
        Error::Script(sheprd_client::Error::ScriptExit { .. })
    ));
    assert!(matches!(
        failure(sheprd_client::Error::ScriptTimeout),
        Error::Script(sheprd_client::Error::ScriptTimeout)
    ));
}

#[test]
fn custom_sockets_have_no_host() {
    assert!(matches!(
        host_for(&ConnectTarget::Socket("/tmp/s".into())),
        Err(Error::UnsupportedHost)
    ));
    assert!(host_for(&ConnectTarget::Local).is_ok());
}

/// `--dev` windows connect to the development installation, which
/// `herdr --session NAME` cannot address: a move would reach the release
/// session of the same name, so none is offered.
#[test]
fn development_sessions_are_refused() {
    assert!(matches!(
        host_for(&ConnectTarget::Session {
            name: "default".into(),
            development: true,
        }),
        Err(Error::DevelopmentSession)
    ));
    assert!(
        host_for(&ConnectTarget::Session {
            name: "default".into(),
            development: false,
        })
        .is_ok()
    );
}

/// The real runner against a stand-in `herdr` that records its arguments:
/// the session is addressed, every argument arrives as one word, and the
/// envelope or exit decides the result.
#[test]
#[cfg(any(target_os = "linux", target_os = "macos"))]
fn runs_the_session_cli_with_quoted_arguments() {
    use std::os::unix::fs::PermissionsExt;
    let home = tempfile::tempdir().unwrap();
    let bin = home.path().join(".local/bin");
    std::fs::create_dir_all(&bin).unwrap();
    let fake = bin.join("herdr");
    std::fs::write(
        &fake,
        r#"#!/bin/sh
for arg in "$@"; do printf '%s\n' "$arg"; done > "$HOME/args"
case "$7" in
  w1:t2) printf '%s\n' '{"result":{"type":"pane_move","move_result":{"changed":true}}}' ;;
  w1:t1) printf '%s\n' '{"result":{"type":"pane_move","move_result":{"changed":false,"reason":"same_tab"}}}' ;;
  *) printf '%s\n' '{"id":"cli:pane:move","error":{"code":"tab_not_found","message":"tab not found"}}' >&2; exit 1 ;;
esac
"#,
    )
    .unwrap();
    std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();
    let mut host = host_for(&ConnectTarget::Session {
        name: "work".into(),
        development: false,
    })
    .unwrap();
    host.env = vec![
        ("HOME".into(), home.path().to_string_lossy().into_owned()),
        ("PATH".into(), "/usr/bin:/bin".into()),
    ];
    let cancelled = AtomicBool::new(false);
    let to =
        |tab: &str| Request::new("w1:p1".into(), Destination::Tab { tab_id: tab.into() }).unwrap();
    to("w1:t2").run(&host, &cancelled).unwrap();
    assert_eq!(
        std::fs::read_to_string(home.path().join("args")).unwrap(),
        "--session\nwork\npane\nmove\nw1:p1\n--tab\nw1:t2\n--split\nright\n--focus\n"
    );
    assert!(matches!(
        to("w1:t1").run(&host, &cancelled),
        Err(Error::Unchanged(Unchanged::SameTab))
    ));
    assert!(matches!(
        to("w1:t 'x").run(&host, &cancelled),
        Err(Error::Refused { code, .. }) if code == "tab_not_found"
    ));
    assert!(
        std::fs::read_to_string(home.path().join("args"))
            .unwrap()
            .contains("\nw1:t 'x\n")
    );
}

#[cfg(unix)]
#[test]
fn a_refusal_is_read_from_the_last_stderr_line_alone() {
    use std::os::unix::process::ExitStatusExt;
    let exit = |stderr: &str| sheprd_client::Error::ScriptExit {
        status: std::process::ExitStatus::from_raw(1 << 8),
        stderr: stderr.into(),
    };
    assert_eq!(
        refused(&exit(
            "warning\n{\"error\":{\"code\":\"agent_blocked\",\"message\":\"asks\"}}\n\n"
        )),
        Some(("agent_blocked".into(), "asks".into()))
    );
    assert_eq!(refused(&exit("usage: herdr agent prompt")), None);
    // An envelope on an earlier line is not this failure's.
    assert_eq!(
        refused(&exit(
            "{\"error\":{\"code\":\"agent_blocked\",\"message\":\"old\"}}\nlater"
        )),
        None
    );
    assert_eq!(refused(&sheprd_client::Error::ScriptTimeout), None);
}
