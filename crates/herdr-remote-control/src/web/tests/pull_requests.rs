use super::*;
use crate::control::{
    Request, Response,
    pull_requests::{PullRequests, Tone, WorkspacePullRequest},
};
use crate::hub::Workspace;
use futures_lite::StreamExt;
use std::time::Duration;

fn badge() -> PullRequests {
    PullRequests {
        boot_id: "b".into(),
        entries: vec![WorkspacePullRequest {
            workspace_id: "w1".into(),
            cwd: "/code/app".into(),
            branch: "topic".into(),
            number: 850,
            url: "https://github.com/penso/herdr-gpui/pull/850".into(),
            tone: Tone::Held,
        }],
    }
}

#[tokio::test]
async fn desktop_pr_changes_update_an_open_mobile_list_and_stale_identities_are_hidden() {
    let dir = tempfile::tempdir().unwrap();
    let hub = Hub::offline(Layout {
        connected: true,
        boot: "b".into(),
        workspaces: vec![Workspace {
            id: "w1".into(),
            label: "Remote control".into(),
            cwd: "/code/app".into(),
            branch: Some("topic".into()),
            chats: Vec::new(),
            terminals: vec![crate::hub::layout_terminal("t1", "Shell")],
        }],
    });
    let state = state_with(dir.path(), hub);
    let (_, token) = signed_in(&state, 3);
    let mut request = HttpRequest::get("/api/stream?view=home")
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    request
        .extensions_mut()
        .insert(ConnectInfo(Peer(SocketAddr::from(([10, 0, 0, 3], 50000)))));
    let response = router(state.clone()).oneshot(request).await.unwrap();
    let mut body = response.into_body().into_data_stream();
    let first = body.next().await.unwrap().unwrap();
    assert!(!String::from_utf8_lossy(&first).contains("#850"));
    let (stop, _) = tokio::sync::watch::channel(false);
    let send = |snapshot| {
        crate::server::answer_for_tests(&state, &stop, Request::PullRequests { snapshot })
    };
    assert_eq!(send(badge()), Response::Done);
    let next = tokio::time::timeout(Duration::from_secs(5), body.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    let next = String::from_utf8_lossy(&next);
    assert!(next.contains("#850") && next.contains("pr-held"), "{next}");
    let html = fragments::lists(&state, false).0.into_string();
    assert!(
        html.split("data-find=\"").skip(1).any(|value| value
            .split('"')
            .next()
            .unwrap()
            .contains("#850"))
    );
    let mut stale = badge();
    stale.boot_id = "old-boot".into();
    assert!(matches!(send(stale), Response::Error { .. }));
    let mut stale = badge();
    stale.entries[0].branch = "other".into();
    assert_eq!(send(stale), Response::Done);
    let html = fragments::lists(&state, false).0.into_string();
    assert!(!html.contains("#850"));
    let mut unsafe_link = badge();
    unsafe_link.entries[0].url = "javascript:alert(1)".into();
    assert!(matches!(send(unsafe_link), Response::Error { .. }));
}
