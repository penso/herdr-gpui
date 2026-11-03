//! The patch stream ends as soon as its phone loses access.

use super::*;
use crate::control::{Request, Response};
use futures_lite::StreamExt;
use std::time::Duration;

#[tokio::test]
async fn revoking_a_phone_ends_its_open_stream() {
    let dir = tempfile::tempdir().unwrap();
    let state = state(dir.path());
    let (device, token) = signed_in(&state, 3);
    let mut request = HttpRequest::get("/api/stream?view=home")
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    request
        .extensions_mut()
        .insert(ConnectInfo(Peer(SocketAddr::from(([10, 0, 0, 3], 50_000)))));
    let response = router(state.clone()).oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let mut body = response.into_body().into_data_stream();
    let first = body.next().await.unwrap().unwrap();
    assert!(String::from_utf8_lossy(&first).contains("event: patch"));
    assert_eq!(state.connected().get(&device), Some(&1));

    let (stop, _) = tokio::sync::watch::channel(false);
    let answer = crate::server::answer_for_tests(&state, &stop, Request::Revoke { device });
    assert!(matches!(answer, Response::Done), "{answer:?}");

    // The rest of the stream is a reload, then its end, without waiting
    // for the periodic check.
    let rest = tokio::time::timeout(Duration::from_secs(5), async {
        let mut rest = String::new();
        while let Some(chunk) = body.next().await {
            rest.push_str(&String::from_utf8_lossy(&chunk.unwrap()));
        }
        rest
    })
    .await
    .unwrap();
    assert!(rest.contains("event: reload"), "{rest}");
    assert!(state.connected().is_empty());
}
