//! Terminal tabs: only known terminal panes are reachable, keys are
//! checked, and actions map to Herdr's API.

use super::*;
use crate::hub::Workspace;

fn with_terminal(dir: &std::path::Path) -> Shared {
    let state = state(dir);
    let layout = Layout {
        connected: true,
        boot: "b".into(),
        workspaces: vec![Workspace {
            id: "w1".into(),
            label: "app".into(),
            branch: None,
            cwd: "/code/app".into(),
            worktree: None,
            chats: vec![crate::hub::ChatRow {
                pane: "c1".into(),
                tab: "t1".into(),
                endpoint: herdr_chat_protocol::EndpointId::random(),
                agent: Some("claude".into()),
                title: "Claude".into(),
                status: herdr_client::protocol::AgentStatus::Idle,
            }],
            terminals: vec![crate::hub::layout_terminal("t2", "zsh")],
        }],
    };
    let state = Arc::try_unwrap(state).ok().unwrap();
    Arc::new(State {
        hub: Hub::offline(layout),
        ..state
    })
}

pub(super) fn authorized(request: HttpRequest<Body>, token: &str) -> HttpRequest<Body> {
    let (mut parts, body) = request.into_parts();
    parts.headers.insert(
        header::AUTHORIZATION,
        HeaderValue::from_str(&format!("Bearer {token}")).unwrap(),
    );
    HttpRequest::from_parts(parts, body)
}

#[test]
fn only_keys_in_herdrs_syntax_pass() {
    for good in [
        "a",
        "A",
        "é",
        "ctrl+c",
        "alt+x",
        "ctrl+alt+d",
        "shift+tab",
        "esc",
        "up",
        "f5",
        "plus",
        "space",
        "-",
        "/",
    ] {
        assert!(crate::web::terminal::valid_key(good), "{good}");
    }
    for bad in [
        "",
        "ctrl+",
        "+",
        "hyper+a",
        "ab",
        "f13",
        "f0",
        "\n",
        " ",
        "ctrl+\u{7}",
        &"x".repeat(40),
        "home",
    ] {
        assert!(!crate::web::terminal::valid_key(bad), "{bad:?}");
    }
}

#[test]
fn actions_call_herdrs_methods_without_focusing() {
    use crate::web::terminal::{Action, action_call};
    let (method, params) = action_call(Action::SplitRight, "w1:p1", "w1:t2", "w1");
    assert_eq!(method, crate::hub::ApiMethod::PaneSplit);
    assert_eq!(params["direction"], "right");
    assert_eq!(params["target_pane_id"], "w1:p1");
    assert_eq!(params["focus"], false);
    assert_eq!(
        action_call(Action::SplitDown, "p", "t", "w").1["direction"],
        "down"
    );
    assert_eq!(action_call(Action::Zoom, "p", "t", "w").1["mode"], "toggle");
    assert_eq!(
        action_call(Action::ClosePane, "p", "t", "w").0,
        crate::hub::ApiMethod::PaneClose
    );
    let (method, params) = action_call(Action::NewTab, "p", "t", "w");
    assert_eq!(
        (method, params["focus"].clone()),
        (crate::hub::ApiMethod::TabCreate, false.into())
    );
    assert_eq!(
        action_call(Action::CloseTab, "p", "t", "w").1["tab_id"],
        "t"
    );
}

#[test]
fn an_action_answers_the_pane_to_show_next() {
    use crate::web::terminal::{Action, next_pane};
    assert_eq!(next_pane(Action::SplitRight, "p1", Some("p2")), Some("p2"));
    assert_eq!(next_pane(Action::NewTab, "p1", Some("p3")), Some("p3"));
    // A split Herdr did not report stays on the pane it split.
    assert_eq!(next_pane(Action::SplitDown, "p1", None), Some("p1"));
    assert_eq!(next_pane(Action::Zoom, "p1", None), Some("p1"));
    assert_eq!(next_pane(Action::ClosePane, "p1", None), None);
    assert_eq!(next_pane(Action::CloseTab, "p1", Some("p9")), None);
}

#[tokio::test]
async fn actions_reach_terminal_and_chat_panes_only() {
    let dir = tempfile::tempdir().unwrap();
    let state = with_terminal(dir.path());
    let (_, token) = signed_in(&state, 7);
    let close = |pane: &str| {
        authorized(
            post(
                &format!("/api/term/{pane}/action"),
                json!({ "action": "close_pane" }),
            ),
            &token,
        )
    };
    assert_eq!(send(&state, close("p9")).await.0, StatusCode::NOT_FOUND);
    // A terminal's pane and a chat's pane both reach Herdr, which this
    // test has none of.
    for pane in ["t2-p1", "c1"] {
        let (status, _, body) = send(&state, close(pane)).await;
        assert_ne!(status, StatusCode::NOT_FOUND, "{pane}: {body}");
        assert_ne!(status, StatusCode::OK, "{pane}: {body}");
    }
}

#[tokio::test]
async fn a_phone_reaches_only_terminal_panes_with_valid_keys() {
    let dir = tempfile::tempdir().unwrap();
    let state = with_terminal(dir.path());
    let (_, token) = signed_in(&state, 4);
    let keys = |pane: &str, keys: serde_json::Value| {
        authorized(
            post(&format!("/api/term/{pane}/keys"), json!({ "keys": keys })),
            &token,
        )
    };
    // Not a terminal pane in the layout.
    assert_eq!(
        send(&state, keys("p9", json!(["a"]))).await.0,
        StatusCode::NOT_FOUND
    );
    // Keys outside Herdr's syntax are refused before reaching Herdr.
    let (status, _, body) = send(&state, keys("t2-p1", json!(["rm -rf"]))).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    let too_many: Vec<&str> = std::iter::repeat_n("a", 65).collect();
    assert_eq!(
        send(&state, keys("t2-p1", json!(too_many))).await.0,
        StatusCode::BAD_REQUEST
    );
    // Without a session nothing is sent.
    let anonymous = post("/api/term/t2-p1/keys", json!({ "keys": ["a"] }));
    assert_eq!(send(&state, anonymous).await.0, StatusCode::UNAUTHORIZED);
    // A known pane with valid keys reaches Herdr, which this test has none of.
    let (status, _, _) = send(&state, keys("t2-p1", json!(["ctrl+c"]))).await;
    assert_ne!(status, StatusCode::OK);
    assert_ne!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn only_terminal_panes_are_read() {
    let dir = tempfile::tempdir().unwrap();
    let state = with_terminal(dir.path());
    let (_, token) = signed_in(&state, 5);
    // A chat's pane, or any pane outside the layout, is refused before
    // Herdr is asked for its screen.
    let (status, _, _) = send(&state, authorized(get("/f/term/p9"), &token)).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _, _) = send(&state, authorized(get("/f/term/t2-p1"), &token)).await;
    assert_ne!(status, StatusCode::NOT_FOUND);
}

#[cfg(unix)]
#[test]
fn a_pasted_picture_is_typed_as_a_dropped_file() {
    use crate::web::terminal::dropped_path;
    let path = std::path::Path::new("/Users/me/Library/Application Support/herdr (1)/a1.png");
    assert_eq!(
        dropped_path(path),
        r"/Users/me/Library/Application\ Support/herdr\ \(1\)/a1.png "
    );
}

#[tokio::test]
async fn pictures_reach_only_terminal_panes() {
    let dir = tempfile::tempdir().unwrap();
    let state = with_terminal(dir.path());
    let (_, token) = signed_in(&state, 6);
    let upload = |pane: &str, bytes: &'static [u8]| {
        let request = HttpRequest::post(format!("/api/term/{pane}/upload"))
            .header(header::CONTENT_TYPE, "image/png")
            .body(Body::from(bytes))
            .unwrap();
        authorized(request, &token)
    };
    let png: &[u8] = b"\x89PNG\r\n\x1a\n rest";
    assert_eq!(
        send(&state, upload("p9", png)).await.0,
        StatusCode::NOT_FOUND
    );
    let (status, _, body) = send(&state, upload("t2-p1", b"not a picture")).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    // A picture for a known pane is stored, then its path goes to Herdr,
    // which this test has none of.
    let (status, _, _) = send(&state, upload("t2-p1", png)).await;
    assert_ne!(status, StatusCode::OK);
    assert_ne!(status, StatusCode::BAD_REQUEST);
    assert_eq!(
        std::fs::read_dir(dir.path().join("uploads"))
            .unwrap()
            .count(),
        1
    );
}
