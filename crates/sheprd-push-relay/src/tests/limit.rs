use super::*;
use crate::limit::{Limiter, source_key};

const RATE: Rate = Rate {
    burst: 2,
    per_minute: 60,
};

#[test]
fn a_burst_then_the_refill_rate() {
    let mut limiter = Limiter::new(RATE);
    assert!(limiter.allow("a", 0));
    assert!(limiter.allow("a", 0));
    assert!(!limiter.allow("a", 0));
    assert!(!limiter.allow("a", 999));
    assert!(limiter.allow("a", 1000));
    assert!(!limiter.allow("a", 1000));
}

#[test]
fn keys_do_not_share_a_bucket() {
    let mut limiter = Limiter::new(tight(1));
    assert!(limiter.allow("a", 0));
    assert!(!limiter.allow("a", 0));
    assert!(limiter.allow("b", 0));
}

#[test]
fn a_clock_that_went_back_grants_nothing() {
    let mut limiter = Limiter::new(RATE);
    assert!(limiter.allow("a", 10_000));
    assert!(limiter.allow("a", 10_000));
    assert!(!limiter.allow("a", 5_000));
}

#[test]
fn the_map_is_bounded_and_keeps_limited_keys() {
    let mut limiter = Limiter::bounded(RATE, 2);
    assert!(limiter.allow("a", 0));
    assert!(limiter.allow("b", 0));
    assert!(!limiter.allow("c", 0), "both slots hold spent buckets");
    // Once a bucket has refilled it is forgotten, and its slot is free.
    assert!(limiter.allow("c", 60_000));
}

#[test]
fn a_new_key_cannot_reset_a_limited_one() {
    let mut limiter = Limiter::bounded(tight(1), 1);
    assert!(limiter.allow("a", 0));
    assert!(!limiter.allow("b", 0));
    assert!(!limiter.allow("a", 0));
}

#[test]
fn sources_group_by_address_or_ipv6_prefix() {
    let key = |text: &str| source_key(text.parse().unwrap());
    assert_eq!(key("2001:db8:1:2:3::1"), key("2001:db8:1:2:ffff::9"));
    assert_ne!(key("2001:db8:1:2::1"), key("2001:db8:1:3::1"));
    assert_ne!(key("203.0.113.1"), key("203.0.113.2"));
    assert_eq!(key("::ffff:203.0.113.1"), key("203.0.113.1"));
}
