//! Sealed alerts: what APNs sees, what the app's extension opens, the
//! 4 KB bound, and the fixture the Swift package opens.

use crate::apns::{
    alert::{MAX_OPTIONS, Sealed, push},
    payload::MAX_PAYLOAD,
    seal::{INFO, seal_with},
    send::{Priority, PushType},
};
use crate::push::{Kind, Notification, Urgency};
use aes_gcm::{Aes256Gcm, KeyInit as _, aead::Aead as _, aead::Payload};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use herdr_chat_protocol::{Permission, PermissionId, PermissionKind, PermissionOption};
use hkdf::Hkdf;
use p256::{PublicKey, SecretKey};
use serde_json::{Value, json};
use sha2::Sha256;
use std::path::PathBuf;

const COMPUTER: &str = "9E92A0DF1CE0967EC34C0C0146005FBC51654330E7D26EC9B5805055755E24F6";

fn device() -> SecretKey {
    SecretKey::from_slice(&[7; 32]).unwrap()
}

/// Seals as production does, with fixed one-time values.
fn fixed_seal(plaintext: &[u8]) -> crate::apns::Result<Vec<u8>> {
    seal_with(
        &device().public_key(),
        plaintext,
        INFO,
        &SecretKey::from_slice(&[9; 32]).unwrap(),
        &[1; 12],
    )
}

/// Opens `sealed` (base64url) with the device key, as the app does.
fn open(sealed: &str) -> Vec<u8> {
    let sealed = URL_SAFE_NO_PAD.decode(sealed).unwrap();
    let ephemeral = PublicKey::from_sec1_bytes(&sealed[..65]).unwrap();
    let mut salt = sealed[..65].to_vec();
    salt.extend_from_slice(&device().public_key().to_sec1_bytes());
    let shared = p256::ecdh::diffie_hellman(device().to_nonzero_scalar(), ephemeral.as_affine());
    let mut key = [0; 32];
    Hkdf::<Sha256>::new(Some(&salt), shared.raw_secret_bytes())
        .expand(INFO, &mut key)
        .unwrap();
    let nonce: [u8; 12] = sealed[65..77].try_into().unwrap();
    Aes256Gcm::new_from_slice(&key)
        .unwrap()
        .decrypt(
            (&nonce).into(),
            Payload {
                msg: &sealed[77..],
                aad: INFO,
            },
        )
        .unwrap()
}

fn option(id: &str, kind: PermissionKind, name: &str) -> PermissionOption {
    PermissionOption {
        id: id.to_owned(),
        kind,
        name: name.to_owned(),
    }
}

fn question() -> Notification {
    Notification {
        body: "Run `git push origin fix/mise`?\nherdr-gpui \u{b7} Fix mise discovery".to_owned(),
        clear: false,
        kind: Kind::Question,
        pane: "w1:p2".to_owned(),
        permission: Some(Permission {
            call_id: None,
            id: PermissionId(7),
            options: vec![
                option("allow", PermissionKind::AllowOnce, "Allow once"),
                option("always", PermissionKind::AllowAlways, "Always allow"),
                option("reject", PermissionKind::RejectOnce, "Deny"),
                option("never", PermissionKind::RejectAlways, "Never"),
                option("fifth", PermissionKind::Other, "Fifth"),
            ],
            title: "Run `git push origin fix/mise`?".to_owned(),
        }),
        tag: "chat-w1:p2".to_owned(),
        title: "Claude needs an answer".to_owned(),
        urgency: Urgency::High,
    }
}

fn json(bytes: &[u8]) -> Value {
    serde_json::from_slice(bytes).unwrap()
}

#[test]
fn a_question_shows_only_a_cover_to_apns() {
    let sealed = Sealed::new(&question(), COMPUTER.to_owned(), 2);
    assert_eq!(
        sealed.permission.as_ref().unwrap().options.len(),
        MAX_OPTIONS
    );
    let push = push(sealed.clone(), "chat-w1:p2", fixed_seal).unwrap();
    assert_eq!(push.push_type, PushType::Alert);
    assert_eq!(push.priority, Priority::High);
    assert_eq!(push.collapse_id.as_deref(), Some("chat-w1:p2"));
    let payload = json(&push.body);
    assert_eq!(
        payload["aps"],
        json!({
            "alert": {"title": "Herdr", "body": "New activity"},
            "mutable-content": 1,
            "thread-id": "w1:p2",
            "interruption-level": "time-sensitive",
            "sound": "default",
        })
    );
    // Nothing of the chat is readable outside the seal.
    let text = String::from_utf8(push.body.clone()).unwrap();
    assert!(!text.contains("git push"));
    let opened: Sealed =
        serde_json::from_slice(&open(payload["sealed"].as_str().unwrap())).unwrap();
    assert_eq!(opened, sealed);
}

#[test]
fn a_finished_turn_is_active_and_low_priority_without_buttons() {
    let notification = Notification {
        kind: Kind::TurnEnded,
        permission: None,
        title: "Claude finished".to_owned(),
        urgency: Urgency::Normal,
        ..question()
    };
    let push = push(
        Sealed::new(&notification, COMPUTER.to_owned(), 0),
        &notification.tag,
        fixed_seal,
    )
    .unwrap();
    assert_eq!(push.priority, Priority::Low);
    let payload = json(&push.body);
    assert_eq!(payload["aps"]["interruption-level"], "active");
    let opened = json(&open(payload["sealed"].as_str().unwrap()));
    assert_eq!(opened["kind"], "turn_ended");
    assert!(opened.get("permission").is_none());
}

#[test]
fn a_clear_is_a_silent_background_push() {
    let notification = Notification {
        body: String::new(),
        clear: true,
        permission: None,
        title: String::new(),
        ..question()
    };
    let push = push(
        Sealed::new(&notification, COMPUTER.to_owned(), 0),
        &notification.tag,
        fixed_seal,
    )
    .unwrap();
    assert_eq!(push.push_type, PushType::Background);
    assert_eq!(push.priority, Priority::Low);
    let payload = json(&push.body);
    // The badge is this computer's count alone, so only the app sets it.
    assert_eq!(payload["aps"], json!({"content-available": 1}));
    let opened = json(&open(payload["sealed"].as_str().unwrap()));
    assert_eq!(opened["badge"], 0);
    assert_eq!(opened["clear"], true);
    assert_eq!(opened["pane"], "w1:p2");
}

#[test]
fn a_long_body_is_shortened_to_fit_apns() {
    let notification = Notification {
        body: "é".repeat(5_000),
        ..question()
    };
    let push = push(
        Sealed::new(&notification, COMPUTER.to_owned(), 1),
        &notification.tag,
        fixed_seal,
    )
    .unwrap();
    assert!(push.body.len() <= MAX_PAYLOAD, "{}", push.body.len());
    let opened: Sealed =
        serde_json::from_slice(&open(json(&push.body)["sealed"].as_str().unwrap())).unwrap();
    assert!(opened.body.ends_with('\u{2026}'));
    assert!(opened.body.chars().count() > 100, "kept as much as fits");
}

#[test]
fn the_collapse_id_stays_within_apns_limit_on_a_character_boundary() {
    let tag = format!("chat-{}", "é".repeat(40));
    let push = push(
        Sealed::new(&question(), COMPUTER.to_owned(), 1),
        &tag,
        fixed_seal,
    )
    .unwrap();
    let id = push.collapse_id.unwrap();
    assert!(id.len() <= 64);
    assert!(tag.starts_with(&id));
}

/// `Fixtures/notify/alert.json`: the device key, the plaintext, and the
/// whole payload, which the Swift package opens. `HERDR_UPDATE_FIXTURES=1`
/// rewrites it.
#[test]
fn the_alert_fixture_opens_to_its_plaintext() {
    let sealed = Sealed::new(&question(), COMPUTER.to_owned(), 2);
    let push = push(sealed.clone(), "chat-w1:p2", fixed_seal).unwrap();
    let fixture = json!({
        "device_private_key": URL_SAFE_NO_PAD.encode(device().to_bytes()),
        "plaintext": sealed,
        "apns": json(&push.body),
    });
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../ios/HerdrRemoteCore/Tests/HerdrRemoteCoreTests/Fixtures/notify/alert.json");
    if std::env::var_os("HERDR_UPDATE_FIXTURES").is_some() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let text = format!("{}\n", serde_json::to_string_pretty(&fixture).unwrap());
        std::fs::write(&path, text).unwrap();
    }
    let expected: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    // Compared as values: key order follows serde_json's features.
    assert_eq!(
        fixture,
        expected,
        "{} is out of date; run with HERDR_UPDATE_FIXTURES=1",
        path.display()
    );
    // The sealed bytes open to exactly the plaintext as serialized.
    let opened = open(expected["apns"]["sealed"].as_str().unwrap());
    assert_eq!(opened, serde_json::to_vec(&sealed).unwrap());
}
