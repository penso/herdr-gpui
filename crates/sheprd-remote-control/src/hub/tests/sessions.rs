//! Named Herdr 1 sessions beside the primary one: their ids, which session a
//! request is for, and how their layouts join the primary's.

use super::*;
use crate::hub::{
    layout::MAX_CHATS,
    layout::project,
    sessions::{listed_with, merge, owner, prefix_answer, prefix_layout, session_name, wanted},
};
// Only the tests that bind Unix sockets or symlinks use these.
#[cfg(unix)]
use crate::hub::sessions::{Sessions, listed as listed_beside};
use sheprd_client::{LocalSession, SessionState};

const ENDPOINT: &str = "0123456789abcdef0123456789abcdef";

/// A chat in `p1`/`t1` and a terminal agent in `p2`/`t2` of workspace `w1`.
fn listed() -> Layout {
    project(&snapshot(json!([
        agent("p1", "t1", json!([["acp", ENDPOINT]])),
        agent("p2", "t2", json!([])),
    ])))
}

#[test]
fn a_named_session_prefixes_every_id_it_lists() {
    let mut layout = listed();
    prefix_layout(&mut layout, "work");
    let workspace = &layout.workspaces[0];
    assert_eq!(workspace.id, "herdr:work:w1");
    assert_eq!(workspace.chats[0].pane, "herdr:work:p1");
    assert_eq!(workspace.chats[0].tab, "herdr:work:t1");
    let terminal = &workspace.terminals[0];
    assert_eq!(terminal.tab, "herdr:work:t2");
    assert_eq!(terminal.panes[0].pane, "herdr:work:p2");
    assert_eq!(terminal.agent.as_ref().unwrap().pane, "herdr:work:p2");
    // What a pane is, as against where it is, stays as Herdr says.
    assert_eq!(workspace.chats[0].endpoint.as_str(), ENDPOINT);
    assert_eq!(layout.boot, "boot-1");
}

#[test]
fn a_request_is_for_the_session_its_ids_name() {
    assert!(owner(&json!({"pane_id": "w1:p1"})).unwrap().is_none());
    assert!(owner(&json!({"cwd": "/code"})).unwrap().is_none());
    let (name, params) = owner(&json!({"pane_id": "herdr:work:w1:p1", "keys": ["Enter"]}))
        .unwrap()
        .unwrap();
    assert_eq!(name, "work");
    assert_eq!(params, json!({"pane_id": "w1:p1", "keys": ["Enter"]}));
    let (_, params) =
        owner(&json!({"target_pane_id": "herdr:work:p2", "workspace_id": "herdr:work:w1"}))
            .unwrap()
            .unwrap();
    assert_eq!(
        params,
        json!({"target_pane_id": "p2", "workspace_id": "w1"})
    );
}

#[test]
fn a_request_naming_two_sessions_or_a_malformed_id_is_refused() {
    for params in [
        json!({"pane_id": "herdr:a:p1", "workspace_id": "herdr:b:w1"}),
        json!({"pane_id": "herdr:a:p1", "workspace_id": "w1"}),
        json!({"pane_id": "herdr:a"}),
        json!({"pane_id": "herdr::p1"}),
        json!({"pane_id": "herdr:a:"}),
    ] {
        assert!(
            matches!(owner(&params), Err(Error::Response)),
            "{params} must be refused"
        );
    }
}

#[test]
fn an_answer_from_a_session_names_its_ids_as_phones_know_them() {
    let mut answer = json!({
        "type": "tab_created",
        "tab": {"tab_id": "t3", "workspace_id": "w1"},
        "root_pane": {"pane_id": "p5", "terminal_id": "term-9"},
        "panes": [{"pane_id": "p6"}],
        "empty": {"pane_id": ""},
    });
    prefix_answer(&mut answer, "work");
    assert_eq!(
        created_tab(&answer),
        Some(("herdr:work:t3".into(), "herdr:work:p5".into()))
    );
    assert_eq!(answer["tab"]["workspace_id"], "herdr:work:w1");
    assert_eq!(answer["panes"][0]["pane_id"], "herdr:work:p6");
    // A terminal is reached through its session's socket, not routed by id.
    assert_eq!(answer["root_pane"]["terminal_id"], "term-9");
    // No id stays no id, so a missing answer is not mistaken for one.
    assert_eq!(answer["empty"]["pane_id"], "");
}

#[test]
fn sessions_join_the_primary_even_when_it_does_not_answer() {
    let primary = Layout::default();
    let session = Arc::new(listed());
    let merged = merge(&primary, &[("work".to_owned(), session.clone())]);
    assert!(merged.connected, "a session answering is Herdr answering");
    assert_eq!(merged.working, session.working);
    assert_eq!(merged.workspaces.len(), 1);
    assert_eq!(merged.workspaces[0].id, "herdr:work:w1");
    // Desktop pull request data is keyed on the primary's boot.
    assert_eq!(merged.boot, "");

    let merged = merge(&listed(), &[("work".to_owned(), session)]);
    assert_eq!(merged.boot, "boot-1");
    let ids: Vec<_> = merged
        .workspaces
        .iter()
        .map(|row| row.id.as_str())
        .collect();
    assert_eq!(ids, ["w1", "herdr:work:w1"]);
}

#[test]
fn a_session_adds_to_an_answering_primary_without_taking_its_boot() {
    let mut primary = listed();
    primary.working = 2;
    let mut session = listed();
    session.connected = false;
    session.working = 3;
    session.boot = "boot-2".into();
    let merged = merge(&primary, &[("work".to_owned(), Arc::new(session))]);
    assert!(merged.connected, "the primary still answers");
    assert_eq!(merged.working, 5);
    assert_eq!(merged.boot, "boot-1");
}

#[test]
fn only_running_sessions_other_than_the_primary_are_followed() {
    let session = |name: &str, state| LocalSession {
        name: name.to_owned(),
        state,
        socket: PathBuf::from(format!("/h/sessions/{name}/herdr-client.sock")),
    };
    let listed = vec![
        session("default", SessionState::Running),
        session("idle", SessionState::Stopped),
        session("pilots", SessionState::Running),
        session("work", SessionState::Running),
    ];
    let primary = PathBuf::from("/h/sessions/pilots/herdr-client.sock");
    let names: Vec<_> = wanted(listed.clone(), Some(&primary))
        .into_iter()
        .map(|(name, _)| name)
        .collect();
    assert_eq!(names, ["default", "work"]);
    assert_eq!(wanted(listed, None).len(), 3);
    // A name `herdr:NAME:` cannot carry is never followed.
    let odd = vec![
        session("a:b", SessionState::Running),
        session("", SessionState::Running),
    ];
    assert!(wanted(odd, None).is_empty());
}

#[test]
fn a_session_is_named_by_where_its_socket_is_in_its_installation() {
    let name = |socket: &str| session_name(Path::new(socket));
    assert_eq!(
        name("/u/.config/herdr/herdr-client.sock").as_deref(),
        Some("default")
    );
    assert_eq!(
        name("/u/.config/herdr/sessions/safeline-pilots/herdr-client.sock").as_deref(),
        Some("safeline-pilots")
    );
    // A sandbox's or a development daemon's socket names no session.
    assert_eq!(name("/tmp/sandbox/herdr-client.sock"), None);
    assert_eq!(name("/u/.config/herdr/h.sock"), None);
    assert_eq!(
        name("/u/.config/herdr/sessions/work/extra/herdr-client.sock"),
        None
    );
}

#[test]
fn each_workspace_names_its_session_through_the_merge() {
    // The primary is a named session, and `coding` is followed beside it.
    let mut base = listed();
    base.name_session("safeline-pilots");
    let core = Workspace {
        id: "core:terminals".into(),
        session: None,
        ..base.workspaces[0].clone()
    };
    base.workspaces.push(core);
    let mut coding = listed();
    coding.name_session("coding");
    let merged = merge(&base, &[("coding".to_owned(), Arc::new(coding))]);
    let sessions: Vec<_> = merged
        .workspaces
        .iter()
        .map(|row| (row.id.as_str(), row.session.as_deref()))
        .collect();
    // A Herdr 2 workspace is in no Herdr 1 session.
    assert_eq!(
        sessions,
        [
            ("w1", Some("safeline-pilots")),
            ("core:terminals", None),
            ("herdr:coding:w1", Some("coding")),
        ]
    );
}

#[cfg(unix)]
#[test]
fn the_primary_is_known_through_a_symlink_to_its_socket() {
    let dir = tempfile::tempdir().unwrap();
    let real = dir.path().join("real");
    std::fs::create_dir(&real).unwrap();
    std::fs::write(real.join("herdr-client.sock"), b"").unwrap();
    std::os::unix::fs::symlink(&real, dir.path().join("link")).unwrap();
    let listed = vec![LocalSession {
        name: "pilots".into(),
        state: SessionState::Running,
        socket: real.join("herdr-client.sock"),
    }];
    let primary = dir.path().join("link").join("herdr-client.sock");
    assert!(wanted(listed, Some(&primary)).is_empty());
}

#[cfg(unix)]
#[test]
fn sessions_are_listed_in_the_installation_the_primary_belongs_to() {
    use std::os::unix::net::UnixListener;

    // Not this process's configuration directory: a service's environment
    // may lack the shell's XDG_CONFIG_HOME, which the primary was resolved in.
    let dir = tempfile::Builder::new()
        .prefix("hl")
        .tempdir_in("/tmp")
        .unwrap();
    let root = dir.path().join("herdr");
    let work = root.join("sessions").join("work");
    std::fs::create_dir_all(&work).unwrap();
    let _work = UnixListener::bind(work.join("herdr-client.sock")).unwrap();
    // The primary is the default session, which is not running here.
    let names: Vec<_> = listed_beside(Some(&root.join("herdr-client.sock")))
        .unwrap()
        .into_iter()
        .map(|(name, _)| name)
        .collect();
    assert_eq!(names, ["work"]);
    // From a named session's socket the installation is three levels up,
    // and the primary itself is not followed twice.
    assert!(
        listed_beside(Some(&work.join("herdr-client.sock")))
            .unwrap()
            .is_empty()
    );
}

#[test]
fn a_socket_outside_an_installation_follows_no_other_session() {
    for primary in [
        "/tmp/sandbox/herdr-client.sock",
        "/home/me/.config/herdr-dev/herdr-client.sock",
        "/home/me/.config/herdr/custom-client.sock",
    ] {
        let listed = listed_with(Some(Path::new(primary)), || {
            panic!("{primary} must not list this process's installation")
        });
        assert!(listed.unwrap().is_empty(), "{primary}");
    }
}

#[test]
fn only_an_unresolved_primary_lists_this_process_s_installation() {
    let names: Vec<_> = listed_with(None, || {
        Ok(vec![LocalSession {
            name: "work".into(),
            state: SessionState::Running,
            socket: PathBuf::from("/nonexistent/herdr-client.sock"),
        }])
    })
    .unwrap()
    .into_iter()
    .map(|(name, _)| name)
    .collect();
    assert_eq!(names, ["work"]);
}

#[test]
fn chats_are_bounded_across_sessions() {
    let full = || {
        let agents: Vec<Value> = (0..MAX_CHATS)
            .map(|index| agent(&format!("p{index}"), "t1", json!([["acp", ENDPOINT]])))
            .collect();
        Arc::new(project(&snapshot(Value::Array(agents))))
    };
    let base = project(&snapshot(json!([agent(
        "p1",
        "t1",
        json!([["acp", ENDPOINT]])
    )])));
    let merged = merge(&base, &[("a".to_owned(), full()), ("b".to_owned(), full())]);
    assert_eq!(merged.chats().count(), MAX_CHATS);
    // The primary's come first, then sessions by name; the last keeps its
    // workspaces, without the chats that did not fit.
    assert_eq!(merged.workspaces[0].chats.len(), 1);
    assert_eq!(merged.workspaces[1].chats.len(), MAX_CHATS - 1);
    assert_eq!(merged.workspaces[2].id, "herdr:b:w1");
    assert!(merged.workspaces[2].chats.is_empty());
}

#[cfg(unix)]
#[test]
fn a_session_that_still_answers_survives_a_missed_listing() {
    let answering = Sessions::offline_with(
        "work",
        listed(),
        PathBuf::from("/nonexistent/h-client.sock"),
    );
    answering.update(&[]);
    assert!(answering.get("work").is_some());
    let quiet = Sessions::offline_with(
        "work",
        Layout::default(),
        PathBuf::from("/nonexistent/h-client.sock"),
    );
    quiet.update(&[]);
    assert!(quiet.get("work").is_none());
}

#[cfg(unix)]
#[tokio::test]
async fn a_session_s_ids_reach_its_own_socket_and_come_back_prefixed() {
    use std::io::{BufRead, BufReader, Write};
    use std::os::unix::net::UnixListener;

    // Short, so the socket path fits SUN_LEN.
    let dir = tempfile::Builder::new()
        .prefix("hs")
        .tempdir_in("/tmp")
        .unwrap();
    let listener = UnixListener::bind(dir.path().join("h.sock")).unwrap();
    let server = std::thread::spawn(move || {
        let mut requests = Vec::new();
        for answer in [
            r#"{"id":"remote","result":{"type":"ok"}}"#,
            r#"{"id":"remote","result":{"type":"workspace_created","workspace":{"workspace_id":"w9"}}}"#,
            r#"{"id":"remote","result":{"pane":{"pane_id":"p1","terminal_id":"term-1"}}}"#,
        ] {
            let (stream, _) = listener.accept().unwrap();
            let mut line = String::new();
            BufReader::new(&stream).read_line(&mut line).unwrap();
            requests.push(serde_json::from_str::<Value>(&line).unwrap());
            (&stream)
                .write_all(format!("{answer}\n").as_bytes())
                .unwrap();
        }
        requests
    });
    let hub = Hub::offline_with_session(listed(), "work", dir.path().join("h-client.sock"));
    let ok = hub
        .call(
            ApiMethod::PaneSendKeys,
            json!({"pane_id": "herdr:work:p1", "keys": ["Enter"]}),
        )
        .await
        .unwrap();
    assert_eq!(ok, json!({"type": "ok"}));
    // The primary does not answer, so a new workspace goes to the session.
    let opened = hub
        .call(
            ApiMethod::WorkspaceCreate,
            json!({"cwd": "/code", "focus": false}),
        )
        .await
        .unwrap();
    assert_eq!(opened["workspace"]["workspace_id"], "herdr:work:w9");
    // Attaching asks the session which terminal the pane shows; there is no
    // terminal to attach to here, so only the question is checked.
    let _ = hub.attach_terminal("herdr:work:p1", 80, 24).await;
    // A session no longer listed cannot be reached.
    let error = hub
        .call(ApiMethod::PaneRead, json!({"pane_id": "herdr:gone:p1"}))
        .await
        .unwrap_err();
    assert!(matches!(error, Error::NotConnected));
    let requests = server.join().unwrap();
    assert_eq!(requests[0]["method"], "pane.send_keys");
    assert_eq!(requests[0]["params"]["pane_id"], "p1");
    assert_eq!(requests[1]["method"], "workspace.create");
    assert_eq!(requests[2]["method"], "pane.get");
    assert_eq!(requests[2]["params"]["pane_id"], "p1");
}

#[cfg(unix)]
#[tokio::test]
async fn a_new_workspace_for_a_session_opens_there_while_the_primary_answers() {
    use std::io::{BufRead, BufReader, Write};
    use std::os::unix::net::UnixListener;

    let dir = tempfile::Builder::new()
        .prefix("hn")
        .tempdir_in("/tmp")
        .unwrap();
    let listener = UnixListener::bind(dir.path().join("h.sock")).unwrap();
    let server = std::thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        let mut line = String::new();
        BufReader::new(&stream).read_line(&mut line).unwrap();
        (&stream)
            .write_all(
                b"{\"id\":\"remote\",\"result\":{\"type\":\"workspace_created\",\"workspace\":{\"workspace_id\":\"w9\"}}}\n",
            )
            .unwrap();
        serde_json::from_str::<Value>(&line).unwrap()
    });
    let mut hub = Hub::offline_with_session(listed(), "work", dir.path().join("h-client.sock"));
    // The primary answers, so a request naming no session would go there:
    // to a socket no one listens on, never this computer's own Herdr.
    let only = Arc::get_mut(&mut hub).unwrap();
    only.primary = watch::channel(Arc::new(listed())).1;
    only.api = api::Api::new(Some(dir.path().join("primary-client.sock")));
    let opened = hub
        .call_in(
            Some("work"),
            ApiMethod::WorkspaceCreate,
            json!({"cwd": "/code", "focus": false}),
        )
        .await
        .unwrap();
    assert_eq!(opened["workspace"]["workspace_id"], "herdr:work:w9");
    assert_eq!(server.join().unwrap()["method"], "workspace.create");
    // The primary's own name is not a session this daemon follows: such a
    // request goes to the primary, whose socket no one listens on here.
    let primary = hub
        .call_in(
            Some("default"),
            ApiMethod::WorkspaceCreate,
            json!({"cwd": "/code", "focus": false}),
        )
        .await;
    assert!(primary.is_err());
}
