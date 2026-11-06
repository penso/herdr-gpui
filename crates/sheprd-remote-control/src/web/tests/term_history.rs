//! Terminal history: read once on request, only from terminal panes, and
//! kept to its newest lines when it is too long to draw whole.

use super::*;
use crate::hub::Workspace;
use serde_json::Value;
use std::{
    io::{BufRead, BufReader, Write},
    os::unix::net::UnixListener,
    sync::mpsc,
};

/// A state whose Herdr API answers every read with `text`, and the requests
/// it took.
fn with_history(dir: &std::path::Path, text: String) -> (Shared, mpsc::Receiver<Value>) {
    let listener = UnixListener::bind(dir.join("h.sock")).unwrap();
    let (requests, taken) = mpsc::channel();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { return };
            let mut line = String::new();
            BufReader::new(&stream).read_line(&mut line).unwrap();
            requests
                .send(serde_json::from_str::<Value>(&line).unwrap())
                .unwrap();
            let answer = json!({"id": "remote", "result": {"read": {"text": text}}});
            writeln!(stream, "{answer}").unwrap();
        }
    });
    let layout = Layout {
        connected: true,
        boot: "b".into(),
        working: 0,
        workspaces: vec![Workspace {
            id: "w1".into(),
            label: "app".into(),
            branch: None,
            cwd: "/code/app".into(),
            worktree: None,
            chats: Vec::new(),
            terminals: vec![crate::hub::layout_terminal("t2", "zsh")],
        }],
    };
    let state = state_with(
        dir,
        Hub::offline_with_api(layout, dir.join("h-client.sock")),
    );
    (state, taken)
}

#[tokio::test]
async fn history_is_the_panes_recent_lines_as_a_screen() {
    let dir = tempfile::tempdir().unwrap();
    let (state, taken) = with_history(dir.path(), "first\n\x1b[31msecond\x1b[0m".to_owned());
    let (_, token) = signed_in(&state, 5);
    let request = terminal::authorized(get("/f/term/t2-p1/history"), &token);
    let (status, _, body) = send(&state, request).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(body.contains(r#"id="term-screen""#), "{body}");
    assert!(body.contains(r#"data-columns="6""#), "{body}");
    assert!(body.contains("first"), "{body}");
    assert!(body.contains("t-fg1"), "{body}");
    let request = taken.try_recv().unwrap();
    assert_eq!(request["method"], "pane.read");
    assert_eq!(
        request["params"],
        json!({
            "pane_id": "t2-p1",
            "source": "recent",
            "lines": 1000,
            "format": "ansi",
        })
    );
}

#[tokio::test]
async fn too_much_history_keeps_its_newest_lines() {
    let dir = tempfile::tempdir().unwrap();
    let old = "old line\n".repeat(64 * 1024);
    let (state, _taken) = with_history(dir.path(), format!("{old}newest line"));
    let (_, token) = signed_in(&state, 5);
    let request = terminal::authorized(get("/f/term/t2-p1/history"), &token);
    let (status, _, body) = send(&state, request).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("newest line"));
    // Cut at a line's start, never partway through one.
    let first = body.split('>').nth(1).unwrap().lines().next().unwrap();
    assert_eq!(first, "old line");
}

#[tokio::test]
async fn history_is_read_only_from_terminal_panes() {
    let dir = tempfile::tempdir().unwrap();
    let (state, taken) = with_history(dir.path(), String::new());
    let (_, token) = signed_in(&state, 5);
    let request = terminal::authorized(get("/f/term/p9/history"), &token);
    let (status, _, _) = send(&state, request).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert!(taken.try_recv().is_err());
}

#[tokio::test]
async fn history_needs_a_signed_in_device() {
    let dir = tempfile::tempdir().unwrap();
    let (state, taken) = with_history(dir.path(), String::new());
    let (status, _, _) = send(&state, get("/f/term/t2-p1/history")).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert!(taken.try_recv().is_err());
}

#[test]
fn history_short_enough_to_draw_is_kept_whole() {
    assert_eq!(crate::ansi::tail("one\ntwo"), "one\ntwo");
    assert_eq!(crate::ansi::tail(""), "");
}
