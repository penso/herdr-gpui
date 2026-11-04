use super::*;

#[test]
fn a_bucket_spends_its_burst_and_refills_one_per_interval() {
    let mut limiter = RateLimiter::new(2, 1_000, 16);
    assert!(limiter.allow("a", T0));
    assert!(limiter.allow("a", T0));
    assert!(!limiter.allow("a", T0));
    assert!(!limiter.allow("a", T0 + 999));
    assert!(limiter.allow("a", T0 + 1_000));
    assert!(!limiter.allow("a", T0 + 1_000));
    // A long rest refills to capacity, not beyond.
    assert!(limiter.allow("a", T0 + 100_000));
    assert!(limiter.allow("a", T0 + 100_000));
    assert!(!limiter.allow("a", T0 + 100_000));
}

#[test]
fn partial_intervals_carry_over() {
    let mut limiter = RateLimiter::new(1, 1_000, 16);
    assert!(limiter.allow("a", T0));
    assert!(!limiter.allow("a", T0 + 600));
    assert!(limiter.allow("a", T0 + 1_000));
    assert!(!limiter.allow("a", T0 + 1_500));
    assert!(limiter.allow("a", T0 + 2_000));
}

#[test]
fn keys_have_their_own_buckets() {
    let mut limiter = RateLimiter::new(1, 1_000, 16);
    assert!(limiter.allow("a", T0));
    assert!(!limiter.allow("a", T0));
    assert!(limiter.allow("b", T0));
}

#[test]
fn the_least_recently_used_key_is_evicted_at_capacity() {
    let mut limiter = RateLimiter::new(1, 60_000, 2);
    assert!(limiter.allow("a", T0));
    assert!(limiter.allow("b", T0 + 1));
    assert!(limiter.allow("c", T0 + 2));
    assert_eq!(limiter.len(), 2);
    // "b" is still remembered and empty; "a" was forgotten.
    assert!(!limiter.allow("b", T0 + 3));
    assert_eq!(limiter.len(), 2);
}

#[test]
fn a_refilled_bucket_is_evicted_before_an_older_spent_one() {
    let mut limiter = RateLimiter::new(3, 1_000, 2);
    assert!(limiter.allow("old", T0));
    assert!(limiter.allow("old", T0));
    assert!(limiter.allow("new", T0 + 500));
    // At 1600 "new" is full again while "old" still owes a request.
    assert!(limiter.allow("third", T0 + 1_600));
    assert_eq!(limiter.len(), 2);
    assert!(limiter.allow("old", T0 + 1_600));
    assert!(limiter.allow("old", T0 + 1_600));
    assert!(!limiter.allow("old", T0 + 1_600));
}

#[test]
fn pairing_claims_are_rate_limited_per_address() {
    let dir = tempfile::tempdir().unwrap();
    let limits = Limits {
        pair_burst: 1,
        pair_refill_ms: 10_000,
        ..Limits::default()
    };
    let mut auth = Auth::load(dir.path(), limits).unwrap();
    let client = IpAddr::from([10, 0, 0, 1]);
    let key = public_key(&signing_key(1));
    let error = auth
        .claim(client, &encode(&[0; 32]), "iPhone", &key, T0)
        .unwrap_err();
    assert!(matches!(error, Error::InvalidToken));
    let offer = auth.mint(T0, &origins(), FINGERPRINT).unwrap();
    let error = auth
        .claim(client, &token(&offer), "iPhone", &key, T0)
        .unwrap_err();
    assert!(matches!(error, Error::RateLimited));
    auth.claim(
        IpAddr::from([10, 0, 0, 2]),
        &token(&offer),
        "iPhone",
        &key,
        T0,
    )
    .unwrap();
}
