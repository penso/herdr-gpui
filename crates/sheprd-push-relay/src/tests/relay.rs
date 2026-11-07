use super::*;
use crate::{
    credential::LIFETIME_S,
    relay::{Reply, RevokeRequest},
    store::{Revoked, Store},
};

fn alert(fixture_handle: String) -> PushRequest {
    request(fixture_handle, PushType::Alert)
}

#[tokio::test]
async fn a_good_push_reaches_the_token_in_its_handle_unchanged() {
    let fixture = fixture();
    let mut push = alert(handle(PHONE, "alert", 1));
    push.collapse_id = Some("chat-w1:p2".to_owned());
    assert_eq!(
        fixture.relay.push(source(), push, NOW_MS).await,
        Reply::Delivered
    );
    let sent = fixture.apple.sent();
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].0, token(PHONE[0]));
    assert_eq!(sent[0].1.body, PAYLOAD.as_bytes());
    assert_eq!(sent[0].1.collapse_id.as_deref(), Some("chat-w1:p2"));
    assert_eq!(sent[0].1.priority, Priority::High);
    assert_eq!(sent[0].1.push_type, PushType::Alert);
}

#[tokio::test]
async fn one_phones_handle_reaches_only_that_phone() {
    let fixture = fixture();
    for device in [PHONE, OTHER_PHONE] {
        let push = alert(handle(device, "alert", 1));
        fixture.relay.push(source(), push, NOW_MS).await;
    }
    let tokens: Vec<_> = fixture
        .apple
        .sent()
        .into_iter()
        .map(|sent| sent.0)
        .collect();
    assert_eq!(tokens, [token(PHONE[0]), token(OTHER_PHONE[0])]);
}

#[tokio::test]
async fn a_bad_handle_is_refused_and_nothing_is_sent() {
    let fixture = fixture();
    let reply = fixture
        .relay
        .push(source(), alert("junk".to_owned()), NOW_MS)
        .await;
    assert_eq!(reply, Reply::Refused);
    assert!(fixture.apple.sent().is_empty());
}

#[tokio::test]
async fn a_forged_credential_is_refused() {
    let fixture = fixture();
    let forged = Issuer::new(&[6; 32])
        .unwrap()
        .issue(PHONE, Host::Production, NOW_S);
    let handle = handle_with(&forged, "alert", &"a".repeat(32), 1);
    let reply = fixture.relay.push(source(), alert(handle), NOW_MS).await;
    assert_eq!(reply, Reply::Refused);
    assert!(fixture.apple.sent().is_empty());
}

#[tokio::test]
async fn a_credential_for_the_other_environment_is_refused() {
    let fixture = fixture();
    let sandbox = issuer().issue(PHONE, Host::Sandbox, NOW_S);
    let handle = handle_with(&sandbox, "alert", &"a".repeat(32), 1);
    let reply = fixture.relay.push(source(), alert(handle), NOW_MS).await;
    assert_eq!(reply, Reply::Refused);
    assert!(fixture.apple.sent().is_empty());
}

#[tokio::test]
async fn a_credential_stops_working_when_it_expires() {
    let fixture = fixture();
    let last = (NOW_S + LIFETIME_S - 1) * 1000;
    let expired = (NOW_S + LIFETIME_S) * 1000;
    let handle = handle(PHONE, "alert", 1);
    let reply = fixture
        .relay
        .push(source(), alert(handle.clone()), last)
        .await;
    assert_eq!(reply, Reply::Delivered);
    let reply = fixture.relay.push(source(), alert(handle), expired).await;
    assert_eq!(reply, Reply::Expired);
    assert_eq!(fixture.apple.sent().len(), 1);
}

#[tokio::test]
async fn revoking_a_handle_stops_it_and_no_other() {
    let fixture = fixture();
    let revoked = handle(PHONE, "alert", 1);
    let kept = handle(PHONE, "alert", 2);
    let request = RevokeRequest {
        handle: revoked.clone(),
    };
    assert_eq!(
        fixture.relay.revoke(source(), request, NOW_MS).await,
        Reply::Revoked
    );
    let reply = fixture
        .relay
        .push(source(), alert(revoked.clone()), NOW_MS)
        .await;
    assert_eq!(reply, Reply::Gone);
    let reply = fixture.relay.push(source(), alert(kept), NOW_MS).await;
    assert_eq!(reply, Reply::Delivered);

    // A daemon that retries its revocation is not told it failed.
    let request = RevokeRequest { handle: revoked };
    assert_eq!(
        fixture.relay.revoke(source(), request, NOW_MS).await,
        Reply::Revoked
    );
}

#[tokio::test]
async fn revoking_needs_a_real_handle() {
    let fixture = fixture();
    let request = RevokeRequest {
        handle: "junk".to_owned(),
    };
    assert_eq!(
        fixture.relay.revoke(source(), request, NOW_MS).await,
        Reply::Refused
    );
}

#[tokio::test]
async fn a_revoked_device_loses_every_handle() {
    let fixture = fixture();
    fixture
        .store
        .revoke(&Revoked::Device(PHONE), NOW_S + 10)
        .await
        .unwrap();
    for id in [1, 2] {
        let push = alert(handle(PHONE, "alert", id));
        assert_eq!(
            fixture.relay.push(source(), push, NOW_MS).await,
            Reply::Gone
        );
    }
    let push = alert(handle(OTHER_PHONE, "alert", 1));
    assert_eq!(
        fixture.relay.push(source(), push, NOW_MS).await,
        Reply::Delivered
    );
}

#[tokio::test]
async fn a_kind_cannot_send_another_kinds_push() {
    let fixture = fixture();
    let push = request(handle(PHONE, "activity", 1), PushType::Alert);
    assert_eq!(
        fixture.relay.push(source(), push, NOW_MS).await,
        Reply::Refused
    );
    let push = request(handle(PHONE, "alert", 1), PushType::LiveActivity);
    assert_eq!(
        fixture.relay.push(source(), push, NOW_MS).await,
        Reply::Refused
    );
    assert!(fixture.apple.sent().is_empty());
}

#[tokio::test]
async fn a_payload_must_be_a_json_object_within_the_bound() {
    let fixture = fixture();
    let padding = "x".repeat(sheprd_apns::MAX_PAYLOAD);
    for bad in [
        "".to_owned(),
        "not json".to_owned(),
        "[1,2]".to_owned(),
        "\"text\"".to_owned(),
        format!(r#"{{"a":"{padding}"}}"#),
    ] {
        let mut push = alert(handle(PHONE, "alert", 1));
        push.payload = bad;
        assert_eq!(
            fixture.relay.push(source(), push, NOW_MS).await,
            Reply::Invalid
        );
    }
    assert!(fixture.apple.sent().is_empty());
}

#[tokio::test]
async fn a_collapse_id_must_fit_a_header() {
    let fixture = fixture();
    for bad in ["", "a b", "a\nb", "é", &"a".repeat(65)] {
        let mut push = alert(handle(PHONE, "alert", 1));
        push.collapse_id = Some(bad.to_owned());
        assert_eq!(
            fixture.relay.push(source(), push, NOW_MS).await,
            Reply::Invalid,
            "{bad:?}"
        );
    }
}

#[tokio::test]
async fn each_apns_answer_maps_to_a_reply() {
    let rejected = |status, reason: &str| Outcome::Rejected {
        reason: Some(reason.to_owned()),
        status,
    };
    let fixture = fixture();
    for (answer, expected) in [
        (Ok(Outcome::Delivered), Reply::Delivered),
        (Ok(Outcome::Gone), Reply::Gone),
        (Ok(Outcome::RateLimited), Reply::Limited),
        (Ok(rejected(400, "BadCollapseId")), Reply::Refused),
        (
            Ok(rejected(403, "InvalidProviderToken")),
            Reply::Unavailable,
        ),
        (Ok(rejected(500, "InternalServerError")), Reply::Unavailable),
        (Ok(rejected(503, "Shutdown")), Reply::Unavailable),
        (Err(sheprd_apns::Error::Timeout), Reply::Unavailable),
    ] {
        fixture.apple.answer(answer);
        let push = alert(handle(PHONE, "alert", 1));
        assert_eq!(fixture.relay.push(source(), push, NOW_MS).await, expected);
    }
}

#[tokio::test]
async fn junk_is_limited_per_source_before_any_handle_is_opened() {
    let fixture = fixture_with(Rates {
        source: tight(2),
        ..Rates::default()
    });
    for expected in [Reply::Refused, Reply::Refused, Reply::Limited] {
        let junk = alert("junk".to_owned());
        assert_eq!(fixture.relay.push(source(), junk, NOW_MS).await, expected);
    }
    // Even a good handle waits now, and another address does not.
    let good = alert(handle(PHONE, "alert", 1));
    assert_eq!(
        fixture.relay.push(source(), good, NOW_MS).await,
        Reply::Limited
    );
    let elsewhere: IpAddr = "198.51.100.1".parse().unwrap();
    let good = alert(handle(PHONE, "alert", 1));
    assert_eq!(
        fixture.relay.push(elsewhere, good, NOW_MS).await,
        Reply::Delivered
    );
}

#[tokio::test]
async fn a_device_is_limited_across_its_handles_and_alone() {
    let fixture = fixture_with(Rates {
        device: tight(2),
        ..Rates::default()
    });
    for (id, expected) in [
        (1, Reply::Delivered),
        (2, Reply::Delivered),
        (3, Reply::Limited),
    ] {
        let push = alert(handle(PHONE, "alert", id));
        assert_eq!(fixture.relay.push(source(), push, NOW_MS).await, expected);
    }
    let push = alert(handle(OTHER_PHONE, "alert", 1));
    assert_eq!(
        fixture.relay.push(source(), push, NOW_MS).await,
        Reply::Delivered
    );
    assert_eq!(fixture.apple.sent().len(), 3);
}

#[tokio::test]
async fn a_handle_is_limited_alone() {
    let fixture = fixture_with(Rates {
        handle: tight(1),
        ..Rates::default()
    });
    for (id, expected) in [
        (1, Reply::Delivered),
        (1, Reply::Limited),
        (2, Reply::Delivered),
    ] {
        let push = alert(handle(PHONE, "alert", id));
        assert_eq!(fixture.relay.push(source(), push, NOW_MS).await, expected);
    }
}

/// What a restart on the memory store means: a relay with an empty store and
/// the same keys takes the handles the daemons already hold.
#[tokio::test]
async fn a_handle_made_before_a_restart_works_after_it() {
    let handle = handle(PHONE, "alert", 1);
    let before = fixture();
    let reply = before.relay.push(source(), alert(handle.clone()), NOW_MS);
    assert_eq!(reply.await, Reply::Delivered);
    drop(before);
    let after = fixture();
    let reply = after.relay.push(source(), alert(handle), NOW_MS);
    assert_eq!(reply.await, Reply::Delivered);
}
