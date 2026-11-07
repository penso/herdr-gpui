use super::*;

const PHONE: &str = "6F9619FF-8B86-D011-B42D-00C04FC964FF";

fn device() -> DeviceId {
    DeviceId("d1".into())
}

fn json(bytes: &[u8]) -> serde_json::Value {
    serde_json::from_slice(bytes).unwrap()
}

fn challenge(nonce: &str) -> Result<Response, control::Error> {
    Ok(Response::PresenceChallenge {
        certificate: "cert".into(),
        nonce: nonce.into(),
        signature: "casig".into(),
    })
}

/// A relay whose phone proved it is `d1`.
fn proven() -> PresenceRelay {
    let now = Instant::now();
    let mut relay = PresenceRelay::default();
    assert_eq!(
        relay.receive(PHONE, br#"{"type":"hello","hint":"h"}"#, now),
        Received::Ask(Request::PresenceChallenge { hint: "h".into() })
    );
    let reply = relay.answered(PHONE, challenge("n"));
    assert_eq!(
        json(&reply),
        serde_json::json!({"type": "challenge", "nonce": "n", "certificate": "cert", "signature": "casig"})
    );
    assert_eq!(
        relay.receive(
            PHONE,
            br#"{"type":"prove","nonce":"n","signature":"s"}"#,
            now
        ),
        Received::Ask(Request::PresenceProve {
            nonce: "n".into(),
            signature: "s".into(),
        })
    );
    let reply = relay.answered(
        PHONE,
        Ok(Response::PresenceProven {
            device: device(),
            addresses: Some("sealed".into()),
        }),
    );
    assert_eq!(
        json(&reply),
        serde_json::json!({"type": "proven", "addresses": "sealed"})
    );
    relay
}

#[test]
fn a_proof_needs_a_nonce_given_to_that_phone() {
    let now = Instant::now();
    let mut relay = PresenceRelay::default();
    let Received::Reply(reply) = relay.receive(
        PHONE,
        br#"{"type":"prove","nonce":"n","signature":"s"}"#,
        now,
    ) else {
        panic!("not refused");
    };
    assert_eq!(json(&reply)["type"], "error");
    relay.receive(PHONE, br#"{"type":"hello","hint":"h"}"#, now);
    relay.answered(PHONE, challenge("n"));
    // Another phone cannot answer this phone's nonce.
    assert!(matches!(
        relay.receive(
            "OTHER",
            br#"{"type":"prove","nonce":"n","signature":"s"}"#,
            now
        ),
        Received::Reply(_)
    ));
    assert!(matches!(
        relay.receive(
            PHONE,
            br#"{"type":"prove","nonce":"other","signature":"s"}"#,
            now
        ),
        Received::Reply(_)
    ));
    // Each nonce is tried once.
    assert!(matches!(
        relay.receive(
            PHONE,
            br#"{"type":"prove","nonce":"n","signature":"s"}"#,
            now
        ),
        Received::Ask(_)
    ));
    assert!(matches!(
        relay.receive(
            PHONE,
            br#"{"type":"prove","nonce":"n","signature":"s"}"#,
            now
        ),
        Received::Reply(_)
    ));
}

#[test]
fn every_failure_reads_the_same_to_the_phone() {
    let now = Instant::now();
    let mut relay = PresenceRelay::default();
    relay.receive(PHONE, br#"{"type":"hello","hint":"h"}"#, now);
    let unknown = relay.answered(
        PHONE,
        Ok(Response::Error {
            code: control::ErrorCode::Other,
            message: "presence was not accepted".into(),
        }),
    );
    let failed = relay.answered(PHONE, Err(control::Error::NoAnswer));
    let Received::Reply(malformed) = relay.receive(PHONE, b"nope", now) else {
        panic!("not refused");
    };
    let expected = serde_json::json!({"type": "error", "message": REFUSED});
    assert_eq!(json(&unknown), expected);
    assert_eq!(json(&failed), expected);
    assert_eq!(json(&malformed), expected);
}

#[test]
fn hellos_are_limited_per_phone() {
    let now = Instant::now();
    let mut relay = PresenceRelay::default();
    for _ in 0..HELLO_LIMIT {
        assert!(matches!(
            relay.receive(PHONE, br#"{"type":"hello","hint":"h"}"#, now),
            Received::Ask(_)
        ));
    }
    assert_eq!(
        relay.receive(PHONE, br#"{"type":"hello","hint":"h"}"#, now),
        Received::Nothing
    );
    // Another phone has its own allowance, and the window moves on.
    assert!(matches!(
        relay.receive("OTHER", br#"{"type":"hello","hint":"h"}"#, now),
        Received::Ask(_)
    ));
    assert!(matches!(
        relay.receive(PHONE, br#"{"type":"hello","hint":"h"}"#, now + HELLO_WINDOW),
        Received::Ask(_)
    ));
}

#[test]
fn near_needs_a_proven_phone_close_by_and_someone_at_the_desk() {
    let now = Instant::now();
    let mut relay = PresenceRelay::default();
    relay.receive(PHONE, br#"{"type":"near","near":true}"#, now);
    relay.idle(5);
    assert!(relay.due(now).is_empty(), "not proven");

    let mut relay = proven();
    relay.receive(PHONE, br#"{"type":"near","near":true}"#, now);
    assert!(relay.due(now).is_empty(), "idle not known yet");
    relay.idle(IDLE_LIMIT_SECONDS);
    assert!(relay.due(now).is_empty(), "nobody at the desk");
    relay.idle(5);
    assert_eq!(
        relay.due(now),
        [Request::PresenceSet {
            device: device(),
            near: true
        }]
    );
    assert!(relay.due(now + Duration::from_secs(1)).is_empty());
    assert_eq!(relay.due(now + RENEW).len(), 1, "renewed");
}

#[test]
fn leaving_or_walking_away_lifts_the_mark_once() {
    let now = Instant::now();
    let mut relay = proven();
    relay.receive(PHONE, br#"{"type":"near","near":true}"#, now);
    relay.idle(5);
    relay.due(now);
    relay.receive(PHONE, br#"{"type":"near","near":false}"#, now);
    assert_eq!(
        relay.due(now),
        [Request::PresenceSet {
            device: device(),
            near: false
        }]
    );
    assert!(relay.due(now + RENEW).is_empty());

    relay.receive(PHONE, br#"{"type":"near","near":true}"#, now);
    relay.due(now);
    relay.disconnected(PHONE);
    assert_eq!(
        relay.due(now),
        [Request::PresenceSet {
            device: device(),
            near: false
        }]
    );
}

#[test]
fn malformed_frames_are_answered_with_an_error() {
    let mut relay = PresenceRelay::default();
    assert!(matches!(
        relay.receive(PHONE, b"nope", Instant::now()),
        Received::Reply(_)
    ));
}
