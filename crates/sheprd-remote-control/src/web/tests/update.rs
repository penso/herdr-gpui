//! `/api/v2/update`: a phone reads where the app's updater stands and asks
//! it to update, which only a running app can take.

use super::*;
use crate::control::app_update::{AppUpdate, Stage};
use serde_json::{Value, json};
use std::time::Instant;

fn with_bearer(token: &str, mut request: HttpRequest<Body>) -> HttpRequest<Body> {
    request.headers_mut().insert(
        header::AUTHORIZATION,
        HeaderValue::from_str(&format!("Bearer {token}")).unwrap(),
    );
    request
}

fn report(state: &Shared) -> bool {
    state.updates().report(
        AppUpdate {
            stage: Stage::Available,
            latest: Some("20261106.1".into()),
            detail: None,
        },
        Instant::now(),
    )
}

#[tokio::test]
async fn updating_needs_a_session() {
    let dir = tempfile::tempdir().unwrap();
    let state = state(dir.path());
    report(&state);
    assert_eq!(
        send(&state, get("/api/v2/update")).await.0,
        StatusCode::UNAUTHORIZED
    );
    let ask = post("/api/v2/update", json!({}));
    assert_eq!(send(&state, ask).await.0, StatusCode::UNAUTHORIZED);
    assert!(!report(&state));
}

#[tokio::test]
async fn a_closed_app_cannot_be_asked() {
    let dir = tempfile::tempdir().unwrap();
    let state = state(dir.path());
    let (_, token) = signed_in(&state, 1);
    let (status, _, body) = send(&state, with_bearer(&token, get("/api/v2/update"))).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        serde_json::from_str::<Value>(&body).unwrap(),
        json!({"app": null, "asked": false})
    );
    let ask = with_bearer(&token, post("/api/v2/update", json!({})));
    assert_eq!(send(&state, ask).await.0, StatusCode::CONFLICT);
}

#[tokio::test]
async fn a_phone_asks_and_the_app_takes_it_with_its_report() {
    let dir = tempfile::tempdir().unwrap();
    let state = state(dir.path());
    let (_, token) = signed_in(&state, 1);
    report(&state);
    let ask = with_bearer(&token, post("/api/v2/update", json!({})));
    let (status, _, body) = send(&state, ask).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        serde_json::from_str::<Value>(&body).unwrap(),
        json!({
            "app": {"stage": "available", "latest": "20261106.1"},
            "asked": true
        })
    );
    assert!(report(&state));
    assert!(!report(&state));
}
