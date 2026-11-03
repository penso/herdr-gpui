//! The Home Screen app takes over a paired browser's device: the page and
//! its manifest carry the token, the browser mints it signed in, and the app redeems
//! it with a key of its own.

use super::*;
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use p256::ecdsa::SigningKey;
use serde_json::{Value, json};

fn bearer_post(path: &str, token: &str) -> HttpRequest<Body> {
    HttpRequest::post(path)
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap()
}

#[tokio::test]
async fn the_manifest_starts_the_app_with_the_token_after_the_hash() {
    let dir = tempfile::tempdir().unwrap();
    let state = state(dir.path());
    let (_, _, plain) = send(&state, get("/manifest.webmanifest")).await;
    let plain: Value = serde_json::from_str(&plain).unwrap();
    assert_eq!(plain["start_url"], "/");
    let (status, _, handed) = send(&state, get("/manifest.webmanifest?handoff=abc_DEF-1")).await;
    assert_eq!(status, StatusCode::OK);
    let handed: Value = serde_json::from_str(&handed).unwrap();
    assert_eq!(handed["start_url"], "/?handoff=abc_DEF-1");
    assert_eq!(handed["name"], plain["name"]);
    // Anything but a token's characters is ignored, never echoed.
    let (_, _, odd) = send(&state, get("/manifest.webmanifest?handoff=%22%3Cx%3E")).await;
    let odd: Value = serde_json::from_str(&odd).unwrap();
    assert_eq!(odd["start_url"], "/");
}

#[tokio::test]
async fn a_page_with_a_token_links_a_manifest_that_carries_it() {
    let dir = tempfile::tempdir().unwrap();
    let state = state(dir.path());
    let (_, _, page) = send(&state, get("/?handoff=abc_DEF-1")).await;
    assert!(
        page.contains(r#"<link rel="manifest" href="/manifest.webmanifest?handoff=abc_DEF-1">"#)
    );
    let (_, _, plain) = send(&state, get("/board")).await;
    assert!(plain.contains(r#"<link rel="manifest" href="/manifest.webmanifest">"#));
    let (_, _, odd) = send(&state, get("/?handoff=%22%3E%3Cscript%3E")).await;
    assert!(odd.contains(r#"<link rel="manifest" href="/manifest.webmanifest">"#));
    assert!(!odd.contains("<script>"));
}

#[tokio::test]
async fn the_home_screen_app_takes_over_the_browsers_device() {
    let dir = tempfile::tempdir().unwrap();
    let state = state(dir.path());
    assert_eq!(
        send(&state, post("/api/handoff", json!({}))).await.0,
        StatusCode::UNAUTHORIZED
    );
    let (device, browser) = signed_in(&state, 5);
    let (status, _, handoff) = send(&state, bearer_post("/api/handoff", &browser)).await;
    assert_eq!(status, StatusCode::OK, "{handoff}");
    let handoff: Value = serde_json::from_str(&handoff).unwrap();

    let app = SigningKey::from_slice(&[6; 32]).unwrap();
    let public = app.verifying_key().to_sec1_point(false);
    let (status, _, redeemed) = send(
        &state,
        post(
            "/api/handoff/redeem",
            json!({"token": handoff["token"], "public_key": URL_SAFE_NO_PAD.encode(public.as_bytes())}),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{redeemed}");
    let redeemed: Value = serde_json::from_str(&redeemed).unwrap();
    assert_eq!(redeemed["device"], device.0);
    assert_eq!(redeemed["origins"], json!(state.origins()));
    // The browser's session ended with its key.
    let request = HttpRequest::get("/f/home")
        .header(header::AUTHORIZATION, format!("Bearer {browser}"))
        .body(Body::empty())
        .unwrap();
    assert_eq!(send(&state, request).await.0, StatusCode::UNAUTHORIZED);
    // Used once.
    let (status, _, _) = send(
        &state,
        post(
            "/api/handoff/redeem",
            json!({"token": handoff["token"], "public_key": URL_SAFE_NO_PAD.encode(public.as_bytes())}),
        ),
    )
    .await;
    assert_ne!(status, StatusCode::OK);
}
