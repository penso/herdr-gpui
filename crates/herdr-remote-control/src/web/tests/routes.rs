use super::*;

#[tokio::test]
async fn the_shell_is_served_with_a_strict_policy() {
    let dir = tempfile::tempdir().unwrap();
    let state = state(dir.path());
    let (status, headers, body) = send(&state, get("/chat/w1:p1")).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("/assets/app.js"));
    let csp = headers[header::CONTENT_SECURITY_POLICY].to_str().unwrap();
    assert!(csp.contains("script-src 'self'"));
    assert!(csp.contains("img-src 'self' data:"));
    assert_eq!(headers[header::X_CONTENT_TYPE_OPTIONS], "nosniff");
}

#[tokio::test]
async fn fragments_and_the_stream_need_a_session() {
    let dir = tempfile::tempdir().unwrap();
    let state = state(dir.path());
    for path in [
        "/f/home",
        "/f/chat/p1",
        "/api/stream?view=home",
        "/api/push/key",
        "/api/origins",
    ] {
        let (status, _, _) = send(&state, get(path)).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "{path}");
    }
    let request = HttpRequest::get("/f/home")
        .header(header::AUTHORIZATION, "Bearer not-a-session")
        .body(Body::empty())
        .unwrap();
    assert_eq!(send(&state, request).await.0, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn another_origin_is_refused_and_ours_gets_cors() {
    let dir = tempfile::tempdir().unwrap();
    let state = state(dir.path());
    let foreign = HttpRequest::post("/api/session")
        .header(header::ORIGIN, "https://evil.example")
        .body(Body::empty())
        .unwrap();
    assert_eq!(send(&state, foreign).await.0, StatusCode::FORBIDDEN);
    let preflight = HttpRequest::options("/f/home")
        .header(header::ORIGIN, ORIGIN)
        .body(Body::empty())
        .unwrap();
    let (status, headers, _) = send(&state, preflight).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert_eq!(headers[header::ACCESS_CONTROL_ALLOW_ORIGIN], ORIGIN);
    assert!(
        headers[header::ACCESS_CONTROL_ALLOW_HEADERS]
            .to_str()
            .unwrap()
            .contains("authorization")
    );
}

#[tokio::test]
async fn only_known_assets_are_served() {
    let dir = tempfile::tempdir().unwrap();
    let state = state(dir.path());
    let (status, headers, _) = send(&state, get("/assets/app.js")).await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        headers[header::CONTENT_TYPE]
            .to_str()
            .unwrap()
            .starts_with("text/javascript")
    );
    assert_eq!(
        send(&state, get("/assets/shell.html")).await.0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        send(&state, get("/assets/..%2Fweb.rs")).await.0,
        StatusCode::NOT_FOUND
    );
    let (status, headers, _) = send(&state, get("/ca.mobileconfig")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        headers[header::CONTENT_TYPE],
        "application/x-apple-aspen-config"
    );
}

#[tokio::test]
async fn a_bad_pairing_token_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let state = state(dir.path());
    let (status, _, body) = send(
        &state,
        post(
            "/api/pair/claim",
            serde_json::json!({"token": "nope", "name": "Phone", "public_key": "x"}),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(body.contains("error"));
}

#[tokio::test]
async fn live_activity_tokens_need_a_session() {
    let dir = tempfile::tempdir().unwrap();
    let state = state(dir.path());
    let (status, _, _) = send(
        &state,
        post("/api/apns", serde_json::json!({"activities": []})),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}
