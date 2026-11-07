use super::*;
use crate::web::{Source, router};
use axum::{
    Router,
    body::Body,
    extract::ConnectInfo,
    http::{Request, StatusCode, header::CONTENT_TYPE},
};
use http_body_util::BodyExt as _;
use std::net::SocketAddr;
use tower::ServiceExt as _;

fn app(rates: Rates, source: Source) -> (Router, Apple) {
    let Fixture { apple, relay, .. } = fixture_with(rates);
    (router(relay, source), apple)
}

fn post(path: &str, body: impl Into<Body>) -> Request<Body> {
    let peer: SocketAddr = "192.0.2.1:4000".parse().unwrap();
    let mut request = Request::post(path)
        .header(CONTENT_TYPE, "application/json")
        .body(body.into())
        .unwrap();
    request.extensions_mut().insert(ConnectInfo(peer));
    request
}

/// A credential issued now, since the routes read the real clock.
fn live_handle(byte: u8) -> String {
    let now_s = crate::web::now_ms() / 1000;
    let credential = issuer().issue(PHONE, Host::Production, now_s);
    handle_with(&credential, "alert", &"a".repeat(32), byte)
}

async fn send(app: &Router, request: Request<Body>) -> (StatusCode, String) {
    let response = app.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (status, String::from_utf8(bytes.to_vec()).unwrap())
}

fn body(handle: String) -> String {
    serde_json::to_string(&request(handle, PushType::Alert)).unwrap()
}

#[tokio::test]
async fn a_push_is_delivered_with_the_wire_names() {
    let (app, apple) = app(Rates::default(), Source::Peer);
    let wire = json!({
        "handle": live_handle(1),
        "payload": PAYLOAD,
        "priority": "high",
        "push_type": "alert",
    });
    let (status, text) = send(&app, post("/v1/push", wire.to_string())).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(text, r#"{"outcome":"delivered"}"#);
    assert_eq!(apple.sent().len(), 1);
}

#[tokio::test]
async fn each_reply_has_its_status() {
    let (app, apple) = app(Rates::default(), Source::Peer);
    let (status, text) = send(&app, post("/v1/push", body("junk".to_owned()))).await;
    assert_eq!(
        (status, text.as_str()),
        (StatusCode::FORBIDDEN, r#"{"outcome":"refused"}"#)
    );

    apple.answer(Ok(Outcome::Gone));
    let (status, _) = send(&app, post("/v1/push", body(live_handle(1)))).await;
    assert_eq!(status, StatusCode::GONE);

    apple.answer(Ok(Outcome::RateLimited));
    let (status, _) = send(&app, post("/v1/push", body(live_handle(1)))).await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);

    apple.answer(Err(sheprd_apns::Error::Timeout));
    let (status, _) = send(&app, post("/v1/push", body(live_handle(1)))).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);

    let old = issuer().issue(PHONE, Host::Production, 0);
    let expired = handle_with(&old, "alert", &"a".repeat(32), 1);
    let (status, _) = send(&app, post("/v1/push", body(expired))).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn a_caller_cannot_choose_the_topic_or_host() {
    let (app, apple) = app(Rates::default(), Source::Peer);
    for field in ["topic", "topic_suffix", "host", "token"] {
        let mut wire = json!({
            "handle": live_handle(1),
            "payload": PAYLOAD,
            "priority": "high",
            "push_type": "alert",
        });
        wire[field] = json!("anything");
        let (status, text) = send(&app, post("/v1/push", wire.to_string())).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{field}");
        assert_eq!(
            text, r#"{"outcome":"invalid"}"#,
            "nothing of it is quoted back"
        );
    }
    assert!(apple.sent().is_empty());
}

#[tokio::test]
async fn malformed_bodies_quote_nothing_back() {
    let (app, _apple) = app(Rates::default(), Source::Peer);
    let (status, text) = send(&app, post("/v1/push", "{\"handle\":\"quoted\"")).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(text, r#"{"outcome":"invalid"}"#);
}

#[tokio::test]
async fn malformed_bodies_spend_the_source_limit() {
    let rates = Rates {
        source: tight(1),
        ..Rates::default()
    };
    let (app, _apple) = app(rates, Source::Peer);
    let (status, _) = send(&app, post("/v1/push", "junk")).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (status, _) = send(&app, post("/v1/push", "junk")).await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
}

#[tokio::test]
async fn bodies_are_bounded() {
    let (app, _apple) = app(Rates::default(), Source::Peer);
    let (status, _) = send(&app, post("/v1/push", "x".repeat(17 * 1024))).await;
    assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE);
}

#[tokio::test]
async fn a_revoke_is_answered() {
    let (app, _apple) = app(Rates::default(), Source::Peer);
    let handle = live_handle(1);
    let wire = json!({ "handle": handle }).to_string();
    let (status, text) = send(&app, post("/v1/revoke", wire)).await;
    assert_eq!(
        (status, text.as_str()),
        (StatusCode::OK, r#"{"outcome":"revoked"}"#)
    );
    let (status, _) = send(&app, post("/v1/push", body(handle))).await;
    assert_eq!(status, StatusCode::GONE);
}

#[tokio::test]
async fn the_health_check_needs_nothing() {
    let (app, _apple) = app(Rates::default(), Source::Peer);
    let request = Request::get("/healthz").body(Body::empty()).unwrap();
    let (status, text) = send(&app, request).await;
    assert_eq!((status, text.as_str()), (StatusCode::OK, "ok"));
}

fn forwarded(value: &str) -> Request<Body> {
    let mut request = post("/v1/push", body("junk".to_owned()));
    request
        .headers_mut()
        .insert("x-forwarded-for", value.parse().unwrap());
    request
}

#[tokio::test]
async fn forwarded_addresses_count_only_behind_a_proxy() {
    let rates = Rates {
        source: tight(1),
        ..Rates::default()
    };
    // Without a proxy, a caller cannot dodge the limit by naming addresses.
    let (app_direct, _apple) = app(rates, Source::Peer);
    let (status, _) = send(&app_direct, forwarded("198.51.100.1")).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, _) = send(&app_direct, forwarded("198.51.100.2")).await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);

    // Behind one, the last entry is what the proxy saw; earlier ones are the
    // caller's own claims.
    let (proxied, _apple) = app(rates, Source::Proxy);
    let (status, _) = send(&proxied, forwarded("1.1.1.1, 198.51.100.1")).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, _) = send(&proxied, forwarded("2.2.2.2, 198.51.100.1")).await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
    let (status, _) = send(&proxied, forwarded("198.51.100.2")).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

/// The daemon's client and this relay must agree on the wire.
#[test]
fn the_daemons_body_is_what_the_relay_reads() {
    use sheprd_apns::{Priority, Push, PushType, push_body};
    let push = Push {
        body: br#"{"aps":{}}"#.to_vec(),
        collapse_id: Some("chat-1".to_owned()),
        priority: Priority::Low,
        push_type: PushType::Background,
    };
    let body = push_body("the-handle", &push).unwrap().unwrap();
    let read: PushRequest = serde_json::from_slice(&body).unwrap();
    assert_eq!(read.handle, "the-handle");
    assert_eq!(read.payload.as_bytes(), push.body);
    assert_eq!(read.collapse_id.as_deref(), Some("chat-1"));
    assert_eq!(read.priority, Priority::Low);
    assert_eq!(read.push_type, PushType::Background);
}
