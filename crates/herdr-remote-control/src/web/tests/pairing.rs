//! Pairing end to end through the router and the control answers, with a
//! phone key made the way WebCrypto makes it.

use super::*;
use crate::control::{PairingState, Request, Response};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use p256::ecdsa::{Signature, SigningKey, signature::Signer};
use serde_json::{Value, json};

fn token_of(url: &str) -> String {
    let fragment = url.split_once('#').unwrap().1;
    fragment
        .split('&')
        .find_map(|part| part.strip_prefix("t="))
        .unwrap()
        .to_owned()
}

fn control(state: &Shared, request: Request) -> Response {
    let (stop, _) = tokio::sync::watch::channel(false);
    crate::server::answer_for_tests(state, &stop, request)
}

#[tokio::test]
async fn a_phone_pairs_with_the_desktops_approval_and_signs_in() {
    let dir = tempfile::tempdir().unwrap();
    let state = state(dir.path());
    let Response::Offer(offer) = control(&state, Request::Pair) else {
        panic!("expected an offer");
    };
    assert!(offer.url.starts_with(ORIGIN));

    let key = SigningKey::from_slice(&[7; 32]).unwrap();
    let public = key.verifying_key().to_sec1_point(false);
    let (status, _, body) = send(
        &state,
        post(
            "/api/pair/claim",
            json!({"token": token_of(&offer.url), "name": "Fabien's iPhone", "public_key": URL_SAFE_NO_PAD.encode(public.as_bytes())}),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let claim: Value = serde_json::from_str(&body).unwrap();
    let code = claim["code"].as_str().unwrap().to_owned();
    let claim_id = claim["claim"].as_str().unwrap().to_owned();

    // The desktop sees the same code and approves.
    let Response::Pairing(PairingState::Claimed {
        device_name,
        code: shown,
        ..
    }) = control(
        &state,
        Request::Pairing {
            id: offer.id.clone(),
        },
    )
    else {
        panic!("expected a claimed pairing");
    };
    assert_eq!(
        (device_name.as_str(), shown.as_str()),
        ("Fabien's iPhone", code.as_str())
    );
    let (_, _, waiting) = send(&state, get(&format!("/api/pair/claim/{claim_id}"))).await;
    assert!(waiting.contains("claimed"));
    control(
        &state,
        Request::Approve {
            id: offer.id,
            allow: true,
            code,
        },
    );

    let (_, _, approved) = send(&state, get(&format!("/api/pair/claim/{claim_id}"))).await;
    let approved: Value = serde_json::from_str(&approved).unwrap();
    assert_eq!(approved["state"], "approved");
    assert_eq!(approved["origins"][0], ORIGIN);
    let device = approved["device"].as_str().unwrap().to_owned();

    // Sign in: the challenge's nonce, signed as WebCrypto signs (P1363).
    let (_, _, challenge) = send(
        &state,
        post("/api/session/challenge", json!({"device": device})),
    )
    .await;
    let challenge: Value = serde_json::from_str(&challenge).unwrap();
    let nonce = URL_SAFE_NO_PAD
        .decode(challenge["nonce"].as_str().unwrap())
        .unwrap();
    let mut message = b"herdr-remote-session\0".to_vec();
    message.extend_from_slice(&nonce);
    let signature: Signature = key.sign(&message);
    let (status, _, session) = send(
        &state,
        post(
            "/api/session",
            json!({"challenge": challenge["challenge"], "signature": URL_SAFE_NO_PAD.encode(signature.to_bytes())}),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{session}");
    let session: Value = serde_json::from_str(&session).unwrap();
    // Each sign-in brings the addresses served now.
    assert_eq!(session["origins"], json!(state.origins()));
    let token = session["token"].as_str().unwrap().to_owned();

    let request = HttpRequest::get("/f/home")
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    let (status, _, home) = send(&state, request).await;
    assert_eq!(status, StatusCode::OK);
    assert!(home.contains("Studio"));

    // A signed-in phone reads the addresses again without a new session.
    let request = HttpRequest::get("/api/origins")
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    let (status, _, origins) = send(&state, request).await;
    assert_eq!(status, StatusCode::OK);
    let origins: Value = serde_json::from_str(&origins).unwrap();
    assert_eq!(origins["origins"], json!(state.origins()));

    // Revoking from the desktop ends the session at once.
    control(
        &state,
        Request::Revoke {
            device: DeviceId(device),
        },
    );
    let request = HttpRequest::get("/f/home")
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    assert_eq!(send(&state, request).await.0, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn registered_live_activity_tokens_are_read_without_deadlock() {
    let dir = tempfile::tempdir().unwrap();
    let state = state(dir.path());
    let Response::Offer(offer) = control(&state, Request::Pair) else {
        panic!("expected an offer");
    };
    let key = SigningKey::from_slice(&[9; 32]).unwrap();
    let public = key.verifying_key().to_sec1_point(false);
    let claim = state
        .auth()
        .claim(
            [10, 0, 0, 9].into(),
            &token_of(&offer.url),
            "iPhone",
            &URL_SAFE_NO_PAD.encode(public.as_bytes()),
            now_ms(),
        )
        .unwrap();
    control(
        &state,
        Request::Approve {
            id: offer.id,
            allow: true,
            code: claim.code,
        },
    );
    let device = state.auth().devices().list(|_| false)[0].id.clone();
    let registration = json!({"push_to_start": "ab".repeat(32), "activities": []});
    state
        .auth()
        .devices_mut()
        .set_apns(&device, Some(registration))
        .unwrap();
    // Reading every registration takes the auth lock once; a nested lock
    // would hang here.
    let found = tokio::time::timeout(std::time::Duration::from_secs(5), async {
        crate::server::registrations_for_tests(&state)
    })
    .await
    .unwrap();
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].0, device);
}
