use super::*;
use crate::{
    auth::Limits,
    hub::Layout,
    tls::{Authority, Names},
};
use axum::{
    body::{Body, to_bytes},
    extract::ConnectInfo,
    http::Request as HttpRequest,
};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use tower::ServiceExt;

mod apps;
mod handoff;
mod indicators;
mod origins;
mod pairing;
mod routes;
mod stream;
#[cfg(unix)]
mod streaming;
mod terminal;
mod uploads;

pub(super) const ORIGIN: &str = "https://192.168.1.20:7443";

/// A server state on a private directory with no daemon behind it.
pub(crate) fn state(dir: &std::path::Path) -> Shared {
    state_with(
        dir,
        Hub::offline(Layout {
            connected: true,
            boot: "b".into(),
            workspaces: Vec::new(),
        }),
    )
}

/// A server state on a private directory showing what `hub` holds.
pub(crate) fn state_with(dir: &std::path::Path, hub: Arc<Hub>) -> Shared {
    let names = Names::new([IpAddr::V4(Ipv4Addr::new(192, 168, 1, 20))], Some("studio"));
    let authority = Authority::load_or_create(dir).unwrap();
    let seen = Origins::load(dir, authority.fingerprint());
    let auth = Auth::load(dir, Limits::default()).unwrap();
    Arc::new(State {
        apns: None,
        apps: auth.apps(),
        auth: Mutex::new(auth),
        computer: "Studio".into(),
        connected: Mutex::new(HashMap::new()),
        fingerprint: authority.fingerprint().to_owned(),
        authority: Some(authority),
        hub,
        look: Look::load(dir),
        names: RwLock::new(names),
        port: 7443,
        privacy: Privacy::Generic,
        push: None,
        reports: Mutex::new(report_limit()),
        seen: Mutex::new(seen),
        uploads: Uploads::new(dir),
    })
}

/// Sends `request` through the router as a phone at 192.168.1.50 would.
pub(super) async fn send(
    state: &Shared,
    mut request: HttpRequest<Body>,
) -> (StatusCode, HeaderMap, String) {
    request
        .extensions_mut()
        .insert(ConnectInfo(Peer(SocketAddr::from((
            [192, 168, 1, 50],
            50_000,
        )))));
    let response = router(state.clone()).oneshot(request).await.unwrap();
    let status = response.status();
    let headers = response.headers().clone();
    let body = to_bytes(response.into_body(), 4 * 1024 * 1024)
        .await
        .unwrap();
    (status, headers, String::from_utf8_lossy(&body).into_owned())
}

/// Pairs a phone whose key comes from `seed` straight through `Auth` and
/// signs it in: its device and bearer token.
pub(crate) fn signed_in(state: &Shared, seed: u8) -> (DeviceId, String) {
    use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
    use p256::ecdsa::{Signature, SigningKey, signature::Signer};
    let key = SigningKey::from_slice(&[seed; 32]).unwrap();
    let public = key.verifying_key().to_sec1_point(false);
    let now = now_ms();
    let mut auth = state.auth();
    let offer = auth.mint(now, &[ORIGIN.to_owned()], "F").unwrap();
    let token = offer
        .url
        .split_once("t=")
        .unwrap()
        .1
        .split('&')
        .next()
        .unwrap();
    let client = IpAddr::V4(Ipv4Addr::new(10, 0, 0, seed));
    let claim = auth
        .claim(
            client,
            token,
            "iPhone",
            &URL_SAFE_NO_PAD.encode(public.as_bytes()),
            now,
        )
        .unwrap();
    auth.answer(&offer.id, true, &claim.code, now).unwrap();
    let device = auth.devices().list(|_| false).last().unwrap().id.clone();
    let challenge = auth.challenge(client, &device, now).unwrap();
    let mut message = b"herdr-remote-session\0".to_vec();
    message.extend_from_slice(&URL_SAFE_NO_PAD.decode(&challenge.nonce).unwrap());
    let signature: Signature = key.sign(&message);
    let session = auth
        .verify(
            &challenge.id,
            &URL_SAFE_NO_PAD.encode(signature.to_bytes()),
            now,
        )
        .unwrap();
    (device, session.token)
}

pub(super) fn get(path: &str) -> HttpRequest<Body> {
    HttpRequest::get(path).body(Body::empty()).unwrap()
}

pub(super) fn post(path: &str, body: serde_json::Value) -> HttpRequest<Body> {
    HttpRequest::post(path)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(body.to_string()))
        .unwrap()
}

#[test]
fn only_this_computers_origins_are_allowed() {
    let origins = vec![ORIGIN.to_owned()];
    assert!(allowed_origin(&origins, ORIGIN));
    assert!(!allowed_origin(&origins, "https://evil.example"));
    assert!(!allowed_origin(
        &origins,
        "https://192.168.1.20:7443.evil.example"
    ));
}

#[test]
fn a_bearer_token_is_read_from_its_header() {
    let mut headers = HeaderMap::new();
    assert_eq!(bearer(&headers), None);
    headers.insert(
        header::AUTHORIZATION,
        HeaderValue::from_static("Bearer abc"),
    );
    assert_eq!(bearer(&headers), Some("abc"));
    headers.insert(header::AUTHORIZATION, HeaderValue::from_static("Basic abc"));
    assert_eq!(bearer(&headers), None);
    headers.insert(header::AUTHORIZATION, HeaderValue::from_static("Bearer "));
    assert_eq!(bearer(&headers), None);
}

#[test]
fn branch_names_follow_gits_rules() {
    for good in ["feat/remote", "fix-1.2", "a_b"] {
        assert!(api::valid_branch(good), "{good}");
    }
    for bad in [
        "-x", "a..b", "a b", "a/", "x.lock", "a@{1}", "/a", "a//b", "a;rm",
    ] {
        assert!(!api::valid_branch(bad), "{bad}");
    }
}
