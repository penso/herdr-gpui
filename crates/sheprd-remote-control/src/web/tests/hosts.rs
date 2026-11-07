//! `/api/v2/hosts`: a phone lists the other hosts it may be paired with
//! from this computer and hands over the key it made for one.

use super::*;
use crate::control::{
    DeviceId,
    hosts::{Enroll, Enrolled, Host, HostId, HostOutcome},
};
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

fn from_page(origin: &str, mut request: HttpRequest<Body>) -> HttpRequest<Body> {
    request
        .headers_mut()
        .insert(header::ORIGIN, HeaderValue::from_str(origin).unwrap());
    request
}

#[tokio::test]
async fn a_page_on_this_computer_is_the_app_a_host_must_admit() {
    let dir = tempfile::tempdir().unwrap();
    let state = state(dir.path());
    known_hosts(&state);
    let (_, token) = signed_in(&state, 1);
    // As `encodeURIComponent` writes it, from this computer's own page: a
    // claim leaves such an origin out, but to the host it is another's.
    let pair = post(
        "/api/v2/hosts/ssh%3Adevbox/pair",
        json!({"public_key": key(5)}),
    );
    let (status, _, _) = send(&state, with_bearer(&token, from_page(ORIGIN, pair))).await;
    assert_eq!(status, StatusCode::OK);
    let taken = state.auth().take_host_pairings(now_ms());
    assert_eq!(taken[0].host, HostId("ssh:devbox".into()));
    assert_eq!(taken[0].app.as_deref(), Some(ORIGIN));
}

#[tokio::test]
async fn the_ios_app_names_no_app() {
    let dir = tempfile::tempdir().unwrap();
    let state = state(dir.path());
    known_hosts(&state);
    let (_, token) = signed_in(&state, 1);
    let pair = post(
        "/api/v2/hosts/ssh:devbox/pair",
        json!({"public_key": key(5)}),
    );
    assert_eq!(
        send(&state, with_bearer(&token, pair)).await.0,
        StatusCode::OK
    );
    assert_eq!(state.auth().take_host_pairings(now_ms())[0].app, None);
}

#[tokio::test]
async fn a_host_answers_the_web_app_enrolled_on_it_and_no_other() {
    let app = "https://mac.tail1234.ts.net:7443";
    let dir = tempfile::tempdir().unwrap();
    let state = state(dir.path());
    let ping = |origin: &str| from_page(origin, get("/api/ping"));
    assert_eq!(send(&state, ping(app)).await.0, StatusCode::FORBIDDEN);
    let device = state
        .auth()
        .enroll(
            &Enroll {
                name: "iPhone".into(),
                public_key: key(5),
                via: "MacBook Pro".into(),
                replace: None,
                app: Some(app.into()),
            },
            now_ms(),
        )
        .unwrap();
    let (status, headers, _) = send(&state, ping(app)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers[header::ACCESS_CONTROL_ALLOW_ORIGIN], app);
    assert_eq!(
        send(&state, ping("https://evil.example")).await.0,
        StatusCode::FORBIDDEN
    );
    assert!(state.auth().revoke(&device).unwrap());
    assert_eq!(send(&state, ping(app)).await.0, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn the_list_gives_a_web_app_what_it_needs_to_keep_a_host() {
    let dir = tempfile::tempdir().unwrap();
    let state = state(dir.path());
    known_hosts(&state);
    let (device, token) = signed_in(&state, 1);
    let pair = post(
        "/api/v2/hosts/ssh:devbox/pair",
        json!({"public_key": key(5)}),
    );
    send(&state, with_bearer(&token, pair)).await;
    state.auth().take_host_pairings(now_ms());
    state
        .auth()
        .host_paired(
            &device,
            &HostId("ssh:devbox".into()),
            None,
            HostOutcome::Paired(Enrolled {
                device: DeviceId("d-host".into()),
                computer: "devbox".into(),
                fingerprint: "AA:BB".into(),
                origins: vec!["https://100.64.0.7:7443".into()],
                app: None,
            }),
            now_ms(),
        )
        .unwrap();
    let (_, _, body) = send(&state, with_bearer(&token, get("/api/v2/hosts"))).await;
    assert_eq!(
        serde_json::from_str::<Value>(&body).unwrap(),
        json!({"hosts": [{
            "host": "ssh:devbox",
            "name": "devbox",
            "state": "paired",
            "device": "d-host",
            "computer": "devbox",
            "fingerprint": "AA:BB",
            "origins": ["https://100.64.0.7:7443"],
        }]})
    );
}

#[test]
fn every_template_the_app_draws_is_in_the_shell() {
    let app = include_str!("../assets/app.js");
    let shell = include_str!("../assets/shell.html");
    let drawn: Vec<&str> = app
        .split('"')
        .filter(|text| {
            text.starts_with("t-")
                && text[2..]
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b == b'-')
        })
        .collect();
    for id in ["t-host-trust", "t-link", "t-trust-address"] {
        assert!(drawn.contains(&id), "{id}");
    }
    for id in drawn {
        assert!(shell.contains(&format!(r#"<template id="{id}">"#)), "{id}");
    }
}
