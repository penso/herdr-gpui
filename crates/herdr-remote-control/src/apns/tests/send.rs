use super::*;
use crate::apns::send::request;

#[test]
fn only_apples_two_hosts() {
    assert_eq!(Host::Production.name(), "api.push.apple.com");
    assert_eq!(Host::Sandbox.name(), "api.sandbox.push.apple.com");
    for (sandbox, host) in [(false, Host::Production), (true, Host::Sandbox)] {
        let request = request(
            &config(sandbox),
            &token(1),
            Priority::Low,
            "jwt",
            Vec::new(),
        )
        .unwrap();
        assert_eq!(request.uri().scheme_str(), Some("https"));
        assert_eq!(request.uri().host(), Some(host.name()));
        assert_eq!(request.uri().port(), None);
        assert_eq!(
            request.uri().path(),
            format!("/3/device/{}", token(1).as_str())
        );
    }
}

#[test]
fn request_carries_the_live_activity_headers() {
    let request = request(
        &config(false),
        &token(1),
        Priority::High,
        "header.claims.signature",
        b"{}".to_vec(),
    )
    .unwrap();
    assert_eq!(request.method(), hyper::Method::POST);
    let header = |name: &str| request.headers()[name].to_str().unwrap().to_owned();
    assert_eq!(header("authorization"), "bearer header.claims.signature");
    assert_eq!(header("apns-push-type"), "liveactivity");
    assert_eq!(
        header("apns-topic"),
        "so.pen.herdr-remote.push-type.liveactivity"
    );
    assert_eq!(header("apns-priority"), "10");
    assert_eq!(header("content-type"), "application/json");
    assert_eq!(Priority::Low.header(), "5");
}

#[test]
fn responses_map_to_outcomes() {
    assert_eq!(outcome(200, b""), Outcome::Delivered);
    assert_eq!(outcome(410, br#"{"reason":"Unregistered"}"#), Outcome::Gone);
    assert_eq!(outcome(410, br#"{"reason":"ExpiredToken"}"#), Outcome::Gone);
    assert_eq!(
        outcome(400, br#"{"reason":"BadDeviceToken"}"#),
        Outcome::Gone
    );
    assert_eq!(
        outcome(400, br#"{"reason":"DeviceTokenNotForTopic"}"#),
        Outcome::Gone
    );
    assert_eq!(
        outcome(429, br#"{"reason":"TooManyRequests"}"#),
        Outcome::RateLimited
    );
    assert_eq!(
        outcome(400, br#"{"reason":"PayloadTooLarge"}"#),
        Outcome::Rejected {
            reason: Some("PayloadTooLarge".to_owned()),
            status: 400
        }
    );
    assert_eq!(
        outcome(403, br#"{"reason":"ExpiredProviderToken"}"#),
        Outcome::Rejected {
            reason: Some("ExpiredProviderToken".to_owned()),
            status: 403
        }
    );
    assert_eq!(
        outcome(500, b"not json"),
        Outcome::Rejected {
            reason: None,
            status: 500
        }
    );
}

#[test]
fn sender_builds_without_touching_the_network() {
    let sender = Sender::new(config(true)).unwrap();
    assert!(format!("{sender:?}").contains("Sender"));
}
