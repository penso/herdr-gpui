use super::*;
use std::time::Duration;

#[test]
fn statuses_map_to_outcomes() {
    assert_eq!(outcome(201, None), Outcome::Delivered);
    assert_eq!(outcome(200, None), Outcome::Delivered);
    assert_eq!(outcome(404, None), Outcome::Gone);
    assert_eq!(outcome(410, None), Outcome::Gone);
    assert_eq!(
        outcome(429, Some(" 30 ")),
        Outcome::RateLimited {
            retry_after: Some(Duration::from_secs(30))
        }
    );
    // An HTTP date is not worth parsing; the caller backs off on its own.
    assert_eq!(
        outcome(429, Some("Wed, 21 Oct 2026 07:28:00 GMT")),
        Outcome::RateLimited { retry_after: None }
    );
    assert_eq!(outcome(400, None), Outcome::Rejected(400));
    assert_eq!(outcome(413, None), Outcome::Rejected(413));
    assert_eq!(outcome(503, None), Outcome::Rejected(503));
}

#[test]
fn topics_are_short_base64url_and_stable_per_tag() {
    let topic = Topic::from_tag("chat-w1-p3");
    assert_eq!(topic.as_str().len(), 32);
    assert!(
        topic
            .as_str()
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
    );
    assert_eq!(topic, Topic::from_tag("chat-w1-p3"));
    assert_ne!(topic, Topic::from_tag("chat-w1-p4"));
    assert!(!topic.as_str().contains("chat"));
}

#[test]
fn delivery_follows_the_notification() {
    let question = Notification {
        body: String::new(),
        clear: false,
        kind: Kind::Question,
        pane: "p".to_owned(),
        tag: "chat-p".to_owned(),
        title: String::new(),
        urgency: Urgency::High,
    };
    let delivery = Delivery::for_notification(&question);
    assert_eq!(delivery.urgency.header(), "high");
    assert_eq!(delivery.ttl_s, 24 * 60 * 60);
    assert_eq!(delivery.topic, Some(Topic::from_tag("chat-p")));

    let clear = Notification {
        clear: true,
        urgency: Urgency::Normal,
        ..question
    };
    let delivery = Delivery::for_notification(&clear);
    assert_eq!(delivery.urgency.header(), "normal");
    assert_eq!(delivery.ttl_s, 60 * 60);
}

#[test]
fn urgency_headers_match_rfc8030() {
    let headers: Vec<_> = [Urgency::Normal, Urgency::High]
        .into_iter()
        .map(Urgency::header)
        .collect();
    assert_eq!(headers, ["normal", "high"]);
}

#[test]
fn a_sender_keeps_its_key() {
    let dir = tempfile::tempdir().unwrap();
    let vapid = Vapid::load_or_create(dir.path()).unwrap();
    let public = vapid.public_key_b64url().to_owned();
    let sender = Sender::new(vapid, "mailto:me@example.com").unwrap();
    assert_eq!(sender.vapid().public_key_b64url(), public);
}

/// Decrypts a push as the browser does, with RFC 8291's receiver key.
fn decrypt(body: &[u8]) -> Vec<u8> {
    use aes_gcm::{Aes128Gcm, KeyInit as _, aead::Aead as _};
    use hkdf::Hkdf;
    use sha2::Sha256;

    let receiver =
        p256::SecretKey::from_slice(&b64("q1dXpw3UpT5VOmu_cf_v6ih07Aems3njxI-JWgLcM94")).unwrap();
    let (salt, rest) = body.split_at(16);
    assert_eq!(rest[..4], 4096u32.to_be_bytes());
    let key_len = usize::from(rest[4]);
    let (sender, ciphertext) = rest[5..].split_at(key_len);
    let sender = p256::PublicKey::from_sec1_bytes(sender).unwrap();
    let shared = receiver.diffie_hellman(&sender);

    let mut info = b"WebPush: info\0".to_vec();
    info.extend_from_slice(&receiver.public_key().to_sec1_bytes());
    info.extend_from_slice(&sender.to_sec1_bytes());
    let mut ikm = [0; 32];
    Hkdf::<Sha256>::new(Some(&b64(AUTH)), shared.raw_secret_bytes())
        .expand(&info, &mut ikm)
        .unwrap();
    let content = Hkdf::<Sha256>::new(Some(salt), &ikm);
    let (mut key, mut nonce) = ([0; 16], [0; 12]);
    content
        .expand(b"Content-Encoding: aes128gcm\0", &mut key)
        .unwrap();
    content
        .expand(b"Content-Encoding: nonce\0", &mut nonce)
        .unwrap();
    let mut record = Aes128Gcm::new_from_slice(&key)
        .unwrap()
        .decrypt(&nonce.into(), ciphertext)
        .unwrap();
    assert_eq!(record.pop(), Some(0x02));
    record
}

fn sender() -> (tempfile::TempDir, Sender) {
    let dir = tempfile::tempdir().unwrap();
    let vapid = Vapid::load_or_create(dir.path()).unwrap();
    (dir, Sender::new(vapid, "mailto:me@example.com").unwrap())
}

#[test]
fn a_prepared_push_carries_the_headers_and_decrypts_to_the_message() {
    let (_dir, sender) = sender();
    let message = Message {
        badge: 1,
        body: "herdr-gpui \u{b7} main".to_owned(),
        clear: false,
        kind: Kind::Question,
        tag: "chat-p".to_owned(),
        title: "Claude needs an answer".to_owned(),
        url: "/chat/p".to_owned(),
    };
    let delivery = Delivery {
        topic: Some(Topic::from_tag("chat-p")),
        ttl_s: 600,
        urgency: Urgency::High,
    };

    let push = sender
        .prepare(&subscription(FCM), &message, &delivery, 1_800_000_000)
        .unwrap();

    let header = |name: &str| {
        push.headers
            .iter()
            .find(|(key, _)| *key == name)
            .map(|(_, value)| value.as_str())
    };
    assert_eq!(header("Content-Encoding"), Some("aes128gcm"));
    assert_eq!(header("TTL"), Some("600"));
    assert_eq!(header("Urgency"), Some("high"));
    assert_eq!(header("Topic"), Some(Topic::from_tag("chat-p").as_str()));
    let authorization = header("Authorization").unwrap();
    assert!(authorization.starts_with("vapid t="));
    assert!(authorization.ends_with(&format!(",k={}", sender.vapid().public_key_b64url())));

    let received: Message = serde_json::from_slice(&decrypt(&push.body)).unwrap();
    assert_eq!(received, message);
}

#[test]
fn oversized_messages_are_refused_before_sending() {
    let (_dir, sender) = sender();
    let message = Message {
        badge: 0,
        body: "x".repeat(MAX_PAYLOAD),
        clear: false,
        kind: Kind::TurnEnded,
        tag: "chat-p".to_owned(),
        title: String::new(),
        url: "/chat/p".to_owned(),
    };
    let delivery = Delivery {
        topic: None,
        ttl_s: 60,
        urgency: Urgency::Normal,
    };
    let error = sender
        .prepare(&subscription(FCM), &message, &delivery, 0)
        .unwrap_err();
    assert!(matches!(error, Error::PayloadTooLarge { .. }), "{error:?}");
}
