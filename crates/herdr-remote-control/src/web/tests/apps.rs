//! One installed app holds several computers: a computer admits the origin
//! of an app installed from another one only once the desktop approves a
//! claim made from it, and only while that device stays paired.

use super::*;
use crate::control::{PairingState, Request, Response};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use p256::ecdsa::SigningKey;
use serde_json::{Value, json};

/// Where the app was installed from: another computer.
const APP: &str = "https://100.64.0.9:7443";

fn control(state: &Shared, request: Request) -> Response {
    let (stop, _) = tokio::sync::watch::channel(false);
    crate::server::answer_for_tests(state, &stop, request)
}

fn from(origin: &str, mut request: HttpRequest<Body>) -> HttpRequest<Body> {
    request
        .headers_mut()
        .insert(header::ORIGIN, HeaderValue::from_str(origin).unwrap());
    request
}

fn token_of(url: &str) -> String {
    url.split_once("#t=")
        .unwrap()
        .1
        .split('&')
        .next()
        .unwrap()
        .to_owned()
}

/// Claims a fresh pairing from `origin`; the offer and the claim's answer.
async fn claim(state: &Shared, origin: &str) -> (crate::control::Offer, StatusCode, Value) {
    let Response::Offer(offer) = control(state, Request::Pair) else {
        panic!("expected an offer");
    };
    let key = SigningKey::from_slice(&[3; 32]).unwrap();
    let public = key.verifying_key().to_sec1_point(false);
    let request = post(
        "/api/pair/claim",
        json!({"token": token_of(&offer.url), "name": "iPhone", "public_key": URL_SAFE_NO_PAD.encode(public.as_bytes())}),
    );
    let (status, _, body) = send(state, from(origin, request)).await;
    (
        offer,
        status,
        serde_json::from_str(&body).unwrap_or(Value::Null),
    )
}

#[tokio::test]
async fn an_app_from_another_computer_is_admitted_once_approved() {
    let dir = tempfile::tempdir().unwrap();
    let state = state(dir.path());
    let ping = || from(APP, get("/api/ping"));
    assert_eq!(send(&state, ping()).await.0, StatusCode::FORBIDDEN);

    let (offer, status, claim) = claim(&state, APP).await;
    assert_eq!(status, StatusCode::OK, "{claim}");
    let status_path = format!("/api/pair/claim/{}", claim["claim"].as_str().unwrap());
    // It may poll its claim, and nothing else, while it waits.
    let (status, headers, _) = send(&state, from(APP, get(&status_path))).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers[header::ACCESS_CONTROL_ALLOW_ORIGIN], APP);
    assert_eq!(send(&state, ping()).await.0, StatusCode::FORBIDDEN);

    control(
        &state,
        Request::Approve {
            id: offer.id,
            allow: true,
            code: claim["code"].as_str().unwrap().to_owned(),
        },
    );
    let (_, _, approved) = send(&state, from(APP, get(&status_path))).await;
    let approved: Value = serde_json::from_str(&approved).unwrap();
    assert_eq!(approved["state"], "approved");
    assert_eq!(send(&state, ping()).await.0, StatusCode::OK);
    // The device keeps its app's origin across a restart.
    let reloaded = Auth::load(dir.path(), Limits::default()).unwrap();
    assert_eq!(*reloaded.apps().read().unwrap(), vec![APP.to_owned()]);

    // Forgetting the device stops admitting its app.
    control(
        &state,
        Request::Revoke {
            device: DeviceId(approved["device"].as_str().unwrap().to_owned()),
        },
    );
    assert_eq!(send(&state, ping()).await.0, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn a_ping_names_the_computer_by_its_fingerprint() {
    let dir = tempfile::tempdir().unwrap();
    let state = state(dir.path());
    let (status, _, body) = send(&state, get("/api/ping")).await;
    assert_eq!(status, StatusCode::OK);
    let ping: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(ping["fingerprint"], state.fingerprint.as_str());
}

#[tokio::test]
async fn the_computers_list_is_served_as_the_shell() {
    let dir = tempfile::tempdir().unwrap();
    let state = state(dir.path());
    let (status, _, body) = send(&state, get("/computers")).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("t-computers"), "{body}");
    let (status, headers, _) = send(&state, get("/assets/jsqr.min.js")).await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        headers[header::CONTENT_TYPE]
            .to_str()
            .unwrap()
            .starts_with("text/javascript")
    );
}

#[tokio::test]
async fn revoking_one_phone_leaves_another_from_the_same_app() {
    let dir = tempfile::tempdir().unwrap();
    let state = state(dir.path());
    let mut devices = Vec::new();
    for _ in 0..2 {
        let (offer, _, claim) = claim(&state, APP).await;
        control(
            &state,
            Request::Approve {
                id: offer.id,
                allow: true,
                code: claim["code"].as_str().unwrap().to_owned(),
            },
        );
        devices.push(
            state
                .auth()
                .devices()
                .list(|_| false)
                .last()
                .unwrap()
                .id
                .clone(),
        );
    }
    let apps: Vec<_> = devices
        .iter()
        .map(|id| state.auth().devices().get(id).unwrap().app.clone())
        .collect();
    assert_eq!(apps, vec![Some(APP.to_owned()), Some(APP.to_owned())]);
    control(
        &state,
        Request::Revoke {
            device: devices[0].clone(),
        },
    );
    assert_eq!(
        send(&state, from(APP, get("/api/ping"))).await.0,
        StatusCode::OK
    );
}

#[tokio::test]
async fn a_denied_claim_admits_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let state = state(dir.path());
    let (offer, _, claim) = claim(&state, APP).await;
    control(
        &state,
        Request::Approve {
            id: offer.id,
            allow: false,
            code: claim["code"].as_str().unwrap().to_owned(),
        },
    );
    assert_eq!(
        send(&state, from(APP, get("/api/ping"))).await.0,
        StatusCode::FORBIDDEN
    );
}

#[tokio::test]
async fn a_claim_from_this_computers_own_page_records_no_app() {
    let dir = tempfile::tempdir().unwrap();
    let state = state(dir.path());
    let (offer, _, claim) = claim(&state, ORIGIN).await;
    control(
        &state,
        Request::Approve {
            id: offer.id,
            allow: true,
            code: claim["code"].as_str().unwrap().to_owned(),
        },
    );
    assert!(state.apps.read().unwrap().is_empty());
}

#[tokio::test]
async fn a_malformed_origin_cannot_claim() {
    let dir = tempfile::tempdir().unwrap();
    let state = state(dir.path());
    for origin in ["http://100.64.0.9:7443", "https://a.example/path", "null"] {
        let (_, status, _) = claim(&state, origin).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{origin}");
    }
}

#[test]
fn only_https_origins_are_app_origins() {
    assert_eq!(api::app_origin(APP).as_deref(), Some(APP));
    assert!(api::app_origin("https://[fd7a:115c::1]:7443").is_some());
    assert!(api::app_origin("https://mac.tail1234.ts.net").is_some());
    for bad in [
        "https://",
        "http://a",
        "https://a/b",
        "https://a?b",
        "https://a b",
    ] {
        assert!(api::app_origin(bad).is_none(), "{bad}");
    }
    assert!(api::app_origin(&format!("https://{}", "a".repeat(260))).is_none());
}

#[tokio::test]
async fn the_desktop_sees_which_app_claimed() {
    let dir = tempfile::tempdir().unwrap();
    let state = state(dir.path());
    let (offer, _, _) = claim(&state, APP).await;
    let Response::Pairing(PairingState::Claimed { app, .. }) =
        control(&state, Request::Pairing { id: offer.id })
    else {
        panic!("expected a claimed pairing");
    };
    assert_eq!(app.as_deref(), Some(APP));

    let (offer, _, _) = claim(&state, ORIGIN).await;
    let Response::Pairing(PairingState::Claimed { app, .. }) =
        control(&state, Request::Pairing { id: offer.id })
    else {
        panic!("expected a claimed pairing");
    };
    assert_eq!(app, None);
}

#[tokio::test]
async fn only_the_claim_paths_are_open_to_any_origin() {
    let dir = tempfile::tempdir().unwrap();
    let state = state(dir.path());
    for path in [
        "/api/pair/claimx",
        "/api/pair/claim/a/b",
        "/api/pair/claim/",
    ] {
        let (status, _, _) = send(&state, from(APP, get(path))).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{path}");
    }
    assert!(is_pair_claim("/api/pair/claim"));
    assert!(is_pair_claim("/api/pair/claim/abc"));
}
