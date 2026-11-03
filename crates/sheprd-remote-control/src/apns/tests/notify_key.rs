//! The alert half of a registration: the app's device token and the key
//! alerts are sealed to.

use super::*;
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use p256::SecretKey;

fn notify_key() -> String {
    URL_SAFE_NO_PAD.encode(
        SecretKey::from_slice(&[5; 32])
            .unwrap()
            .public_key()
            .to_sec1_bytes(),
    )
}

#[test]
fn an_alert_token_and_notify_key_register_for_sealed_alerts() {
    let body = format!(
        r#"{{"activities": [], "alert": "{}", "notify_key": "{}"}}"#,
        "9f".repeat(32),
        notify_key()
    );
    let registration = Registration::parse(body.as_bytes()).unwrap();
    let (token, key) = registration.alerts().unwrap();
    assert_eq!(token.as_str(), "9f".repeat(32));
    assert_eq!(*key, SecretKey::from_slice(&[5; 32]).unwrap().public_key());
    // Stored and read back as the app sent it.
    let stored = registration.to_stored().unwrap().unwrap();
    assert_eq!(stored["notify_key"], notify_key());
    assert_eq!(Registration::from_stored(Some(&stored)), registration);
}

#[test]
fn a_notify_key_that_is_not_an_uncompressed_point_is_refused() {
    let compressed = URL_SAFE_NO_PAD.encode(
        SecretKey::from_slice(&[5; 32])
            .unwrap()
            .public_key()
            .to_sec1_bytes()
            .iter()
            .take(33)
            .copied()
            .collect::<Vec<u8>>(),
    );
    for key in [
        "not base64!",
        "AAAA",
        compressed.as_str(),
        &URL_SAFE_NO_PAD.encode([4; 65]),
    ] {
        let body = format!(r#"{{"notify_key": "{key}"}}"#);
        assert!(Registration::parse(body.as_bytes()).is_err(), "{key}");
    }
    assert!(matches!(
        crate::apns::tokens::NotifyKey::parse("AAAA"),
        Err(Error::NotifyKey)
    ));
}

#[test]
fn alerts_need_both_halves_and_an_update_keeps_what_it_omits() {
    let mut stored = Registration {
        alert: Some(token(9)),
        ..Registration::default()
    };
    assert!(stored.alerts().is_none(), "no key yet");
    assert!(!stored.is_empty());
    stored.merge(
        Registration::parse(format!(r#"{{"notify_key": "{}"}}"#, notify_key()).as_bytes()).unwrap(),
    );
    assert!(stored.alerts().is_some());
    // A Live Activity update without them keeps both.
    stored.merge(Registration::parse(br#"{"activities": []}"#).unwrap());
    assert!(stored.alerts().is_some());
}

#[test]
fn a_refused_alert_token_is_forgotten() {
    let mut stored = Registration {
        alert: Some(token(9)),
        ..Registration::default()
    };
    assert!(stored.forget(&token(9)));
    assert!(stored.alert.is_none());
    assert!(!stored.forget(&token(9)));
}
