//! New workspaces and chats: what the forms offer, and the checks made
//! before anything is created.

use super::*;
use crate::native::create::{CreatedWorkspace, NewOptions};
use crate::web::tests::{post, send, signed_in};
use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use herdr_chat_protocol::MAX_PROMPT_SIZE;
use serde_json::json;

/// A state whose one workspace sits in `root/app`, beside `root/lib`.
fn with_folders(dir: &std::path::Path) -> (Shared, String, String) {
    let root = dir.join("code");
    for name in ["app", "lib", ".hidden"] {
        std::fs::create_dir_all(root.join(name)).unwrap();
    }
    let app = root.join("app").to_str().unwrap().to_owned();
    let mut layout = layout(EndpointId::random());
    layout.workspaces[0].cwd = app.clone();
    let state = offline(&dir.join("state"), layout);
    (state, app, root.to_str().unwrap().to_owned())
}

fn authorized(mut request: Request<Body>, token: &str) -> Request<Body> {
    request.headers_mut().insert(
        header::AUTHORIZATION,
        format!("Bearer {token}").parse().unwrap(),
    );
    request
}

#[tokio::test]
async fn the_form_offers_every_agent_and_the_folders_beside_workspaces() {
    let dir = tempfile::tempdir().unwrap();
    let (state, app, root) = with_folders(dir.path());
    let (_, token) = signed_in(&state, 7);
    let request = Request::get("/api/v2/new").body(Body::empty()).unwrap();
    let (status, _, body) = send(&state, authorized(request, &token)).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let options: NewOptions = serde_json::from_str(&body).unwrap();
    let ids: Vec<&str> = options
        .agents
        .iter()
        .map(|agent| agent.id.as_str())
        .collect();
    let expected: Vec<&str> = herdr_chat_protocol::Agent::ALL
        .iter()
        .map(|agent| agent.name())
        .collect();
    assert_eq!(ids, expected);
    assert_eq!(options.recent, vec![app.clone()]);
    assert_eq!(options.roots.len(), 1);
    assert_eq!(options.roots[0].path, root);
    assert!(options.roots[0].folders.contains(&app));
    assert!(
        !options.roots[0]
            .folders
            .iter()
            .any(|folder| folder.ends_with(".hidden"))
    );
}

#[tokio::test]
async fn creating_needs_a_session() {
    let dir = tempfile::tempdir().unwrap();
    let (state, app, _) = with_folders(dir.path());
    for request in [
        post("/api/v2/workspaces", json!({"cwd": app, "agent": "claude"})),
        post("/api/v2/workspaces/w1/chats", json!({"agent": "claude"})),
        Request::get("/api/v2/new").body(Body::empty()).unwrap(),
    ] {
        let (status, _, _) = send(&state, request).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }
}

#[tokio::test]
async fn a_workspace_request_is_checked_before_anything_is_created() {
    let dir = tempfile::tempdir().unwrap();
    let (state, app, root) = with_folders(dir.path());
    let (_, token) = signed_in(&state, 7);
    let long = "x".repeat(MAX_PROMPT_SIZE + 1);
    for (body, error) in [
        (json!({"cwd": app, "agent": "nope"}), "unknown agent"),
        (
            json!({"cwd": format!("{root}/../etc"), "agent": "claude"}),
            "that is not a folder you can start a workspace in",
        ),
        (
            json!({"cwd": app, "agent": "claude", "branch": "a..b"}),
            "that is not a valid branch name",
        ),
        (
            json!({"cwd": app, "agent": "claude", "prompt": long}),
            "the prompt is too long",
        ),
    ] {
        let request = authorized(post("/api/v2/workspaces", body), &token);
        let (status, _, answer) = send(&state, request).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{answer}");
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&answer).unwrap()["error"],
            error
        );
    }
    // A valid request reaches the hub, which has no Herdr behind it here.
    let request = authorized(
        post(
            "/api/v2/workspaces",
            json!({"cwd": app, "agent": "claude", "branch": "feat/x", "prompt": "Hi"}),
        ),
        &token,
    );
    let (status, _, _) = send(&state, request).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
}

#[tokio::test]
async fn a_chat_request_is_checked_before_anything_is_created() {
    let dir = tempfile::tempdir().unwrap();
    let (state, _, _) = with_folders(dir.path());
    let (_, token) = signed_in(&state, 7);
    let long = "x".repeat(MAX_PROMPT_SIZE + 1);
    for body in [
        json!({"agent": "nope"}),
        json!({"agent": "claude", "prompt": long}),
    ] {
        let request = authorized(post("/api/v2/workspaces/w1/chats", body), &token);
        let (status, _, answer) = send(&state, request).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{answer}");
    }
    let request = authorized(
        post("/api/v2/workspaces/w1/chats", json!({"agent": "codex"})),
        &token,
    );
    let (status, _, _) = send(&state, request).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
}

#[test]
fn the_forms_json_matches_the_fixtures() {
    fixtures::check(
        "new",
        &NewOptions::new(
            vec!["/Users/me/code/herdr-gpui".into()],
            vec![(
                "/Users/me/code".into(),
                vec![
                    "/Users/me/code/herdr-gpui".into(),
                    "/Users/me/code/moltis".into(),
                ],
            )],
        ),
    );
    fixtures::check(
        "created",
        &CreatedWorkspace {
            pane: Some("p7".into()),
            prompted: true,
            workspace: "w3".into(),
        },
    );
}
