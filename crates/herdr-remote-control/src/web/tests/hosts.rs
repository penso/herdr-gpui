//! `/api/v2/hosts`: a phone lists the other hosts it may be paired with
//! from this computer and hands over the key it made for one.

use super::*;
use crate::control::hosts::{Host, HostId};
use serde_json::{Value, json};

fn with_bearer(token: &str, mut request: HttpRequest<Body>) -> HttpRequest<Body> {
    request.headers_mut().insert(
        header::AUTHORIZATION,
        HeaderValue::from_str(&format!("Bearer {token}")).unwrap(),
    );
    request
}

fn key(seed: u8) -> String {
    use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
    let key = p256::ecdsa::SigningKey::from_slice(&[seed; 32]).unwrap();
    URL_SAFE_NO_PAD.encode(key.verifying_key().to_sec1_point(false).as_bytes())
}

fn known_hosts(state: &Shared) {
    state
        .auth()
        .set_hosts(vec![Host {
            id: HostId("ssh:devbox".into()),
            name: "devbox".into(),
        }])
        .unwrap();
}

#[tokio::test]
async fn hosts_need_a_session() {
    let dir = tempfile::tempdir().unwrap();
    let state = state(dir.path());
    assert_eq!(
        send(&state, get("/api/v2/hosts")).await.0,
        StatusCode::UNAUTHORIZED
    );
    let pair = post(
        "/api/v2/hosts/ssh:devbox/pair",
        json!({"public_key": key(5)}),
    );
    assert_eq!(send(&state, pair).await.0, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn a_phone_lists_hosts_and_asks_for_one() {
    let dir = tempfile::tempdir().unwrap();
    let state = state(dir.path());
    known_hosts(&state);
    let (_, token) = signed_in(&state, 1);
    let (status, _, body) = send(&state, with_bearer(&token, get("/api/v2/hosts"))).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        serde_json::from_str::<Value>(&body).unwrap(),
        json!({"hosts": [{"host": "ssh:devbox", "name": "devbox", "state": "offered"}]})
    );
    // Not allowed when the phone was paired: the desktop is asked.
    let pair = post(
        "/api/v2/hosts/ssh:devbox/pair",
        json!({"public_key": key(5)}),
    );
    let (status, _, body) = send(&state, with_bearer(&token, pair)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        serde_json::from_str::<Value>(&body).unwrap(),
        json!({"state": "asking"})
    );
    // Another phone sees only its own standing.
    let (_, other) = signed_in(&state, 2);
    let (_, _, body) = send(&state, with_bearer(&other, get("/api/v2/hosts"))).await;
    assert_eq!(
        serde_json::from_str::<Value>(&body).unwrap()["hosts"][0]["state"],
        "offered"
    );
}

#[tokio::test]
async fn unknown_hosts_and_bad_keys_are_refused() {
    let dir = tempfile::tempdir().unwrap();
    let state = state(dir.path());
    known_hosts(&state);
    let (_, token) = signed_in(&state, 1);
    let elsewhere = post(
        "/api/v2/hosts/ssh:elsewhere/pair",
        json!({"public_key": key(5)}),
    );
    assert_eq!(
        send(&state, with_bearer(&token, elsewhere)).await.0,
        StatusCode::NOT_FOUND
    );
    let bad = post(
        "/api/v2/hosts/ssh:devbox/pair",
        json!({"public_key": "nope"}),
    );
    assert_eq!(
        send(&state, with_bearer(&token, bad)).await.0,
        StatusCode::BAD_REQUEST
    );
}
