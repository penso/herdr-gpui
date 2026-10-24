#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;
use base64::Engine as _;

mod encrypt;
mod send;
mod subscription;
mod throttle;
mod triggers;
mod vapid;

/// RFC 8291 section 5's receiver keys, which the fixtures subscribe with.
const UA_PUBLIC: &str =
    "BCVxsr7N_eNgVRqvHtD0zTZsEc6-VV-JvLexhqUzORcxaOzi6-AYWXvTBHm4bjyPjs7Vd8pZGH6SRpkNtoIAiw4";
const AUTH: &str = "BTBZMqHH6r4Tts7J_aSIgg";
const FCM: &str = "https://fcm.googleapis.com/fcm/send/abc:def";

fn b64(text: &str) -> Vec<u8> {
    super::vapid::B64.decode(text).unwrap()
}

fn subscription(endpoint: &str) -> Subscription {
    Subscription::parse(endpoint, UA_PUBLIC, AUTH).unwrap()
}
