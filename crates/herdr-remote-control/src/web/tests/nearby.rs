//! Nearby pairing through the router and the control socket: the code
//! exchange, the claim that carries its code, and the desktop's approval.

use super::*;
use crate::{
    auth::nearby::{KEY_INFO, code, commit, fingerprint_bytes, key, transcript},
    control::{NearbyRevealed, NearbyStarted, PairingState, Request, Response},
};
use aes_gcm::{
    Aes256Gcm, KeyInit as _,
    aead::{Aead as _, Payload},
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use p256::{SecretKey, ecdsa::SigningKey};
use serde_json::{Value, json};

fn control(state: &Shared, request: Request) -> Response {
    let (stop, _) = tokio::sync::watch::channel(false);
    crate::server::answer_for_tests(state, &stop, request)
}

/// The phone: a fixed key and nonce, and the checks it makes.
struct Phone(SecretKey);

impl Phone {
    fn public_key(&self) -> String {
        URL_SAFE_NO_PAD.encode(self.0.public_key().to_sec1_bytes())
    }

    /// Checks the commitment and opens the token: the code and the token.
    fn finish(&self, started: &NearbyStarted, revealed: &NearbyRevealed) -> (String, String) {
        let decode = |text: &str| URL_SAFE_NO_PAD.decode(text).unwrap();
        let daemon_key = decode(&started.public_key);
        let daemon_nonce = decode(&revealed.nonce);
        let phone_key = self.0.public_key().to_sec1_bytes();
        assert_eq!(
            URL_SAFE_NO_PAD.encode(commit(&daemon_key, &phone_key, &daemon_nonce)),
            started.commit
        );
        let fingerprint = fingerprint_bytes(&started.fingerprint).unwrap();
        let transcript = transcript(
            &phone_key,
            &daemon_key,
            &[9; 32],
            &daemon_nonce,
            &fingerprint,
        );
        let point = p256::PublicKey::from_sec1_bytes(&daemon_key).unwrap();
        let shared = p256::ecdh::diffie_hellman(self.0.to_nonzero_scalar(), point.as_affine());
        let key = key(shared.raw_secret_bytes(), &transcript).unwrap();
        let sealed = decode(&revealed.sealed);
        let nonce: [u8; 12] = sealed[..12].try_into().unwrap();
        let plaintext = Aes256Gcm::new_from_slice(key.as_slice())
            .unwrap()
            .decrypt(
                (&nonce).into(),
                Payload {
                    msg: &sealed[12..],
                    aad: KEY_INFO,
                },
            )
            .unwrap();
        let contents: Value = serde_json::from_slice(&plaintext).unwrap();
        (
            code(&transcript),
            contents["token"].as_str().unwrap().to_owned(),
        )
    }
}

fn device_key() -> String {
    let key = SigningKey::from_slice(&[7; 32]).unwrap();
    URL_SAFE_NO_PAD.encode(key.verifying_key().to_sec1_point(false).as_bytes())
}

#[tokio::test]
async fn starting_without_a_pairing_dialog_is_a_conflict() {
    let dir = tempfile::tempdir().unwrap();
    let state = state(dir.path());
    let phone = Phone(SecretKey::from_slice(&[3; 32]).unwrap());
    let (status, _, body) = send(
        &state,
        post(
            "/api/nearby/start",
            json!({"public_key": phone.public_key()}),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
}

#[tokio::test]
async fn a_phone_pairs_nearby_over_the_network_and_the_desktop_sees_its_code() {
    let dir = tempfile::tempdir().unwrap();
    let state = state(dir.path());
    let Response::Offer(offer) = control(&state, Request::Pair) else {
        panic!("expected an offer");
    };
    let phone = Phone(SecretKey::from_slice(&[3; 32]).unwrap());
    let (status, _, body) = send(
        &state,
        post(
            "/api/nearby/start",
            json!({"public_key": phone.public_key()}),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let started: NearbyStarted = serde_json::from_str(&body).unwrap();
    assert_eq!(started.computer, "Studio");
    assert_eq!(started.fingerprint, state.fingerprint.replace(':', ""));

    let (status, _, body) = send(
        &state,
        post(
            "/api/nearby/reveal",
            json!({"session": started.session, "nonce": URL_SAFE_NO_PAD.encode([9; 32])}),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let revealed: NearbyRevealed = serde_json::from_str(&body).unwrap();
    let (code, token) = phone.finish(&started, &revealed);

    let (status, _, body) = send(
        &state,
        post(
            "/api/pair/claim",
            json!({"token": token, "name": "iPhone", "public_key": device_key()}),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let claim: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(claim["code"], code);
    let Response::Pairing(PairingState::Claimed { code: shown, .. }) = control(
        &state,
        Request::Pairing {
            id: offer.id.clone(),
        },
    ) else {
        panic!("expected a claim");
    };
    assert_eq!(shown, code);
    let approved = control(
        &state,
        Request::Approve {
            id: offer.id,
            allow: true,
            code,
            hosts: Vec::new(),
        },
    );
    assert!(
        matches!(approved, Response::Pairing(PairingState::Approved { .. })),
        "{approved:?}"
    );
}

#[tokio::test]
async fn an_unknown_session_is_not_found_and_any_origin_may_ask() {
    let dir = tempfile::tempdir().unwrap();
    let state = state(dir.path());
    let mut request = post(
        "/api/nearby/reveal",
        json!({"session": "nope", "nonce": URL_SAFE_NO_PAD.encode([9; 32])}),
    );
    request.headers_mut().insert(
        header::ORIGIN,
        HeaderValue::from_static("https://elsewhere.example"),
    );
    let (status, headers, body) = send(&state, request).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    assert_eq!(
        headers[header::ACCESS_CONTROL_ALLOW_ORIGIN],
        "https://elsewhere.example"
    );
}

#[test]
fn herdr_ui_relays_the_exchange_over_the_control_socket() {
    let dir = tempfile::tempdir().unwrap();
    let state = state(dir.path());
    let phone = Phone(SecretKey::from_slice(&[3; 32]).unwrap());
    let refused = control(
        &state,
        Request::NearbyStart {
            public_key: phone.public_key(),
        },
    );
    assert!(
        matches!(
            refused,
            Response::Error {
                code: crate::control::ErrorCode::NoPairing,
                ..
            }
        ),
        "{refused:?}"
    );
    let Response::Offer(_) = control(&state, Request::Pair) else {
        panic!("expected an offer");
    };
    let Response::NearbyStarted(started) = control(
        &state,
        Request::NearbyStart {
            public_key: phone.public_key(),
        },
    ) else {
        panic!("expected the first step");
    };
    let Response::NearbyRevealed(revealed) = control(
        &state,
        Request::NearbyReveal {
            session: started.session.clone(),
            nonce: URL_SAFE_NO_PAD.encode([9; 32]),
        },
    ) else {
        panic!("expected the second step");
    };
    let (_, token) = phone.finish(&started, &revealed);
    let claim = state
        .auth()
        .claim(
            "10.0.0.9".parse().unwrap(),
            &token,
            "iPhone",
            &device_key(),
            now_ms(),
        )
        .unwrap();
    assert_eq!(claim.code.len(), 6);
}

#[test]
fn the_control_messages_are_tagged_as_herdr_ui_reads_them() {
    let request = serde_json::to_value(Request::NearbyReveal {
        session: "s".into(),
        nonce: "n".into(),
    })
    .unwrap();
    assert_eq!(
        request,
        json!({"request": "nearby_reveal", "session": "s", "nonce": "n"})
    );
    let response = serde_json::to_value(Response::NearbyRevealed(NearbyRevealed {
        nonce: "n".into(),
        sealed: "x".into(),
    }))
    .unwrap();
    assert_eq!(
        response,
        json!({"response": "nearby_revealed", "nonce": "n", "sealed": "x"})
    );
}
