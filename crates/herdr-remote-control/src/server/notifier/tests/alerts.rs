//! Which devices get sealed APNs alerts rather than Web Push.

use super::*;
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use p256::SecretKey;
use serde_json::json;

fn with_apns(dir: &std::path::Path) -> Arc<State> {
    let mut state = crate::web::tests::state(dir);
    Arc::get_mut(&mut state).unwrap().apns = Some(apns::sender_for_tests());
    state
}

fn register(state: &State, device: &DeviceId, registration: serde_json::Value) {
    state
        .auth()
        .devices_mut()
        .set_apns(device, Some(registration))
        .unwrap();
}

fn alerts() -> serde_json::Value {
    let key = SecretKey::from_slice(&[5; 32])
        .unwrap()
        .public_key()
        .to_sec1_bytes();
    json!({"alert": "9f".repeat(32), "notify_key": URL_SAFE_NO_PAD.encode(key)})
}

#[test]
fn a_device_registered_for_alerts_gets_them_through_apns() {
    let dir = tempfile::tempdir().unwrap();
    let state = with_apns(dir.path());
    let (alerted, _) = crate::web::tests::signed_in(&state, 3);
    let (activities_only, _) = crate::web::tests::signed_in(&state, 4);
    register(&state, &alerted, alerts());
    register(
        &state,
        &activities_only,
        json!({"push_to_start": "ab".repeat(32)}),
    );
    // Neither has a Web Push subscription; only the first is offered
    // notifications, and through APNs.
    assert_eq!(alert_devices(&state), vec![alerted]);
}

#[test]
fn without_an_apns_key_nobody_gets_alerts_through_apns() {
    let dir = tempfile::tempdir().unwrap();
    let state = crate::web::tests::state(dir.path());
    let (device, _) = crate::web::tests::signed_in(&state, 3);
    register(&state, &device, alerts());
    assert!(alert_devices(&state).is_empty());
}

#[test]
fn a_token_apns_refused_is_dropped_from_the_device() {
    let dir = tempfile::tempdir().unwrap();
    let state = with_apns(dir.path());
    let (device, _) = crate::web::tests::signed_in(&state, 3);
    register(&state, &device, alerts());
    let token = apns::DeviceToken::parse(&"9f".repeat(32)).unwrap();
    forget(&state, &device, &token);
    assert!(alert_devices(&state).is_empty());
}
