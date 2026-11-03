#![allow(clippy::unwrap_used)]
use super::*;

mod contention;

// Tests that write the process-wide transport cache coordinate explicitly.
static SERIAL: Mutex<()> = Mutex::new(());

fn known(token: &SecretString) -> Option<Quota> {
    let Observation::Ready(quota) = latest(token) else {
        panic!("unexpected contended quota read")
    };
    quota
}

fn headers() -> ureq::http::HeaderMap {
    let mut headers = ureq::http::HeaderMap::new();
    for (name, value) in [
        ("x-ratelimit-limit", "5000"),
        ("x-ratelimit-remaining", "29"),
        ("x-ratelimit-reset", "2000000000"),
    ] {
        headers.insert(name, value.parse().unwrap());
    }
    headers
}

#[test]
fn captures_points_from_headers_and_keeps_credentials_separate() {
    let _serial = SERIAL.lock().unwrap();
    let first = SecretString::from("quota-first");
    let second = SecretString::from("quota-second");
    record(&first, &headers());
    let quota = known(&first).unwrap();
    assert_eq!((quota.used, quota.limit, quota.remaining), (4971, 5000, 29));
    assert_eq!(quota.reset.unwrap().timestamp(), 2000000000);
    assert_eq!(known(&second), None);
    let mut headers = headers();
    headers.insert("x-ratelimit-remaining", "0".parse().unwrap());
    record(&first, &headers);
    assert_eq!(known(&first).unwrap().used, 5000);
}

#[test]
fn absent_or_invalid_quota_is_not_fabricated() {
    assert_eq!(parse(&ureq::http::HeaderMap::new()), None);
    let mut headers = headers();
    headers.insert("x-ratelimit-remaining", "5001".parse().unwrap());
    assert_eq!(parse(&headers), None);
    headers.insert("x-ratelimit-remaining", "invalid".parse().unwrap());
    assert_eq!(parse(&headers), None);
}

#[test]
fn exhausted_quota_waits_only_until_its_reported_reset() {
    let reset = DateTime::from_timestamp(2_000_000_000, 0).unwrap();
    let mut quota = Quota {
        limit: 5000,
        used: 5000,
        remaining: 0,
        reset: Some(reset),
    };
    let now = reset - chrono::TimeDelta::minutes(29);
    assert_eq!(
        quota.retry_after(now),
        Some(std::time::Duration::from_secs(1740))
    );
    assert_eq!(quota.retry_after(reset), None);
    assert_eq!(
        quota.retry_after(reset + chrono::TimeDelta::seconds(1)),
        None
    );
    quota.remaining = 1;
    assert_eq!(quota.retry_after(now), None);
    quota.remaining = 0;
    quota.reset = None;
    assert_eq!(quota.retry_after(now), None);
}

#[test]
fn reported_usage_can_exceed_the_allowance() {
    let mut headers = headers();
    headers.insert("x-ratelimit-remaining", "0".parse().unwrap());
    headers.insert("x-ratelimit-used", "5001".parse().unwrap());
    assert_eq!(parse(&headers).unwrap().used, 5001);
}

#[test]
fn an_account_publishes_transport_usage_and_drops_it_when_signed_out() {
    let _serial = SERIAL.lock().unwrap();
    let mut auth = crate::Auth::connected_fixture();
    let token = std::sync::Arc::new(SecretString::from("quota-account-poll"));
    auth.profile.as_mut().unwrap().token = token.clone();
    record(&token, &headers());
    assert!(auth.poll());
    assert_eq!(auth.quota().unwrap().remaining, 29);
    auth.profile = None;
    assert_eq!(auth.quota(), None);
    assert!(auth.poll());
    assert_eq!(auth.quota(), None);
}
