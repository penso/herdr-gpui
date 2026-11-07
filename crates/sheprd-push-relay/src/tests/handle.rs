use super::*;
use crate::{Kind, handle::MAX_HANDLE};
use secrecy::ExposeSecret as _;

const ID: &str = "0123456789abcdef0123456789abcdef";

fn opened(id: &str, kind: &str) -> serde_json::Value {
    json!({ "credential": "c", "id": id, "kind": kind, "token": token_hex(9) })
}

fn ring() -> Keyring {
    Keyring::new(relay_key(), None)
}

#[test]
fn opens_what_the_phone_sealed() {
    let text = seal_to(
        &relay_key(),
        &opened(ID, "push_to_start"),
        seal::HANDLE_INFO,
    );
    let handle = ring().open(&text).unwrap();
    assert_eq!(handle.credential.expose_secret(), "c");
    assert_eq!(handle.id, ID);
    assert_eq!(handle.kind, Kind::PushToStart);
    assert_eq!(handle.token, token(9));
}

#[test]
fn a_previous_key_still_opens_its_handles() {
    let old = SecretKey::from_slice(&[9; 32]).unwrap();
    let text = seal_to(&old, &opened(ID, "alert"), seal::HANDLE_INFO);
    assert!(ring().open(&text).is_none());
    assert!(Keyring::new(relay_key(), Some(old)).open(&text).is_some());
}

#[test]
fn something_sealed_for_another_purpose_is_no_handle() {
    for info in [seal::INFO, seal::ADDRESSES_INFO] {
        let text = seal_to(&relay_key(), &opened(ID, "alert"), info);
        assert!(ring().open(&text).is_none());
    }
}

#[test]
fn another_keys_handle_under_this_keys_id_is_refused() {
    let other = SecretKey::from_slice(&[9; 32]).unwrap();
    let text = seal_to(&other, &opened(ID, "alert"), seal::HANDLE_INFO);
    let (_, sealed) = text.split_once('.').unwrap();
    let forged = format!("{}.{sealed}", key_id(&relay_key()));
    assert!(ring().open(&forged).is_none());
}

#[test]
fn malformed_handles_are_refused() {
    let good = seal_to(&relay_key(), &opened(ID, "alert"), seal::HANDLE_INFO);
    let long = format!("{good}{}", "A".repeat(MAX_HANDLE));
    let no_key = good.split_once('.').unwrap().1.to_owned();
    let unknown = format!("00000000.{no_key}");
    let not_base64 = format!("{}.!!!", key_id(&relay_key()));
    for bad in ["", ".", "x.y", &long, &no_key, &unknown, &not_base64] {
        assert!(ring().open(bad).is_none(), "{bad}");
    }
}

#[test]
fn contents_are_checked_too() {
    let bad_token = json!({ "credential": "c", "id": ID, "kind": "alert", "token": "zz" });
    let extra = json!({
        "credential": "c", "id": ID, "kind": "alert", "token": token_hex(9), "topic": "x",
    });
    for contents in [
        opened("short", "alert"),
        opened(&ID.to_uppercase(), "alert"),
        opened(ID, "voip"),
        bad_token,
        extra,
    ] {
        let text = seal_to(&relay_key(), &contents, seal::HANDLE_INFO);
        assert!(ring().open(&text).is_none(), "{contents}");
    }
}

#[test]
fn each_kind_allows_only_its_push_types() {
    use PushType::{Alert, Background, LiveActivity};
    for (kind, push_type, allowed) in [
        (Kind::Activity, LiveActivity, true),
        (Kind::Activity, Alert, false),
        (Kind::Activity, Background, false),
        (Kind::PushToStart, LiveActivity, true),
        (Kind::PushToStart, Alert, false),
        (Kind::PushToStart, Background, false),
        (Kind::Alert, LiveActivity, false),
        (Kind::Alert, Alert, true),
        (Kind::Alert, Background, true),
    ] {
        assert_eq!(kind.allows(push_type), allowed, "{kind:?} {push_type:?}");
    }
}
