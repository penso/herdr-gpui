//! Typing reads the screen back at once rather than on the next timer
//! read, so a phone sees its echo sooner.

use super::*;
use crate::hub::Workspace;
use std::{
    io::{BufRead, BufReader, Write},
    os::unix::net::UnixListener,
    time::Duration,
};
use tokio::time::timeout;

/// Shorter than the regular screen interval, so only an echo read fits.
const SOONER: Duration = Duration::from_millis(200);

/// A state whose Herdr API answers every request with an empty result.
fn with_api(dir: &std::path::Path) -> Shared {
    let listener = UnixListener::bind(dir.join("h.sock")).unwrap();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { return };
            let mut line = String::new();
            BufReader::new(&stream).read_line(&mut line).unwrap();
            stream
                .write_all(b"{\"id\":\"remote\",\"result\":{}}\n")
                .unwrap();
        }
    });
    let layout = Layout {
        connected: true,
        boot: "b".into(),
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
    state_with(
        dir,
        Hub::offline_with_api(layout, dir.join("h-client.sock")),
    )
}

#[tokio::test]
async fn keys_herdr_took_read_the_screen_at_once() {
    let dir = tempfile::tempdir().unwrap();
    let state = with_api(dir.path());
    let (_, token) = signed_in(&state, 7);
    let mut pace = state.typed.pace("t2-p1");
    let request = post("/api/term/t2-p1/keys", json!({ "keys": ["l", "s"] }));
    let (status, _, body) = send(&state, terminal::authorized(request, &token)).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    timeout(SOONER, pace.wait())
        .await
        .expect("an echo read is due at once");
}

#[tokio::test]
async fn keys_herdr_never_took_leave_the_timer_alone() {
    let dir = tempfile::tempdir().unwrap();
    let state = state_with(
        dir.path(),
        Hub::offline(Layout {
            connected: true,
            boot: "b".into(),
            workspaces: vec![Workspace {
                id: "w1".into(),
                label: "app".into(),
                branch: None,
                cwd: "/code/app".into(),
                worktree: None,
                chats: Vec::new(),
                terminals: vec![crate::hub::layout_terminal("t2", "zsh")],
            }],
        }),
    );
    let (_, token) = signed_in(&state, 8);
    let mut pace = state.typed.pace("t2-p1");
    let request = post("/api/term/t2-p1/keys", json!({ "keys": ["a"] }));
    let (status, _, _) = send(&state, terminal::authorized(request, &token)).await;
    assert_ne!(status, StatusCode::OK);
    assert!(timeout(SOONER, pace.wait()).await.is_err());
}
