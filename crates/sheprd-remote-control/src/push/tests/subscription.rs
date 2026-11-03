use super::*;
use crate::push::subscription::{MAX_ENDPOINT, is_push_service};
use p256::elliptic_curve::sec1::ToSec1Point as _;

#[test]
fn browser_json_round_trips() {
    let json = serde_json::json!({
        "endpoint": FCM,
        "expirationTime": null,
        "keys": { "p256dh": UA_PUBLIC, "auth": AUTH },
    });
    let parsed: Subscription = serde_json::from_value(json).unwrap();
    assert_eq!(parsed.endpoint(), FCM);
    assert_eq!(parsed.audience(), "https://fcm.googleapis.com");
    let back = serde_json::to_value(&parsed).unwrap();
    assert_eq!(
        back,
        serde_json::json!({ "endpoint": FCM, "keys": { "p256dh": UA_PUBLIC, "auth": AUTH } })
    );
}

#[test]
fn debug_hides_the_endpoint_path() {
    let text = format!("{:?}", subscription(FCM));
    assert!(text.contains("fcm.googleapis.com"));
    assert!(!text.contains("abc:def"));
}

#[test]
fn known_push_services_are_accepted() {
    for endpoint in [
        FCM,
        "https://android.googleapis.com/gcm/send/x",
        "https://web.push.apple.com/QGx",
        "https://updates.push.services.mozilla.com/wpush/v2/x",
        "https://other.push.services.mozilla.com/x",
        "https://wns2-par02p.notify.windows.com/w/?token=x",
    ] {
        assert!(
            Subscription::parse(endpoint, UA_PUBLIC, AUTH).is_ok(),
            "{endpoint}"
        );
    }
}

#[test]
fn other_hosts_are_refused() {
    for host in [
        "push.apple.com",
        "evil.com",
        "fcm.googleapis.com.evil.com",
        "evilpush.apple.com",
        "googleapis.com",
        "localhost",
    ] {
        let endpoint = format!("https://{host}/x");
        let error = Subscription::parse(&endpoint, UA_PUBLIC, AUTH).unwrap_err();
        assert!(
            matches!(error, Error::EndpointHost { .. }),
            "{host}: {error:?}"
        );
    }
    let error = Subscription::parse("https://127.0.0.1/x", UA_PUBLIC, AUTH).unwrap_err();
    assert!(matches!(error, Error::EndpointHost { .. }), "{error:?}");
    assert!(!is_push_service(".push.apple.com"));
}

#[test]
fn only_plain_https_is_accepted() {
    for endpoint in [
        "http://fcm.googleapis.com/fcm/send/x",
        "https://fcm.googleapis.com:8443/fcm/send/x",
        "https://user:pass@fcm.googleapis.com/fcm/send/x",
    ] {
        let error = Subscription::parse(endpoint, UA_PUBLIC, AUTH).unwrap_err();
        assert!(
            matches!(error, Error::EndpointScheme),
            "{endpoint}: {error:?}"
        );
    }
    let error = Subscription::parse("not a url", UA_PUBLIC, AUTH).unwrap_err();
    assert!(matches!(error, Error::Endpoint(_)), "{error:?}");
}

#[test]
fn long_endpoints_are_refused() {
    let base = "https://fcm.googleapis.com/fcm/send/";
    let fits = format!("{base}{}", "a".repeat(MAX_ENDPOINT - base.len()));
    assert!(Subscription::parse(&fits, UA_PUBLIC, AUTH).is_ok());
    let long = format!("{fits}a");
    let error = Subscription::parse(&long, UA_PUBLIC, AUTH).unwrap_err();
    assert!(matches!(error, Error::EndpointTooLong { len } if len == MAX_ENDPOINT + 1));
}

#[test]
fn bad_keys_are_refused() {
    let short_auth = "BTBZMqHH6r4Tts7J_aSI";
    let error = Subscription::parse(FCM, UA_PUBLIC, short_auth).unwrap_err();
    assert!(matches!(error, Error::AuthSecret), "{error:?}");
    let error = Subscription::parse(FCM, UA_PUBLIC, "!!!").unwrap_err();
    assert!(matches!(error, Error::AuthSecret), "{error:?}");

    // A compressed point, a point off the curve, and not base64 at all.
    let compressed = &super::super::vapid::B64.encode(
        p256::PublicKey::from_sec1_bytes(&b64(UA_PUBLIC))
            .unwrap()
            .to_sec1_point(true)
            .as_bytes(),
    );
    let mut off_curve = b64(UA_PUBLIC);
    off_curve[64] ^= 1;
    let off_curve = super::super::vapid::B64.encode(off_curve);
    for p256dh in [compressed.as_str(), off_curve.as_str(), "***"] {
        let error = Subscription::parse(FCM, p256dh, AUTH).unwrap_err();
        assert!(matches!(error, Error::PublicKey), "{p256dh}: {error:?}");
    }
}
