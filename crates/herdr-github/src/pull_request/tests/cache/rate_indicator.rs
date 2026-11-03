use super::*;

#[test]
fn indicator_counts_down_expires_and_is_scoped_to_the_account() {
    let mut peer = Peer::new();
    let now = Instant::now();
    peer.cache.schedule([input("limited")], now);
    peer.cache.poll(now);
    peer.complete(
        now,
        Err(Error::RateLimit { reset: None }),
        Some(Duration::from_secs(120)),
    );
    assert_eq!(
        peer.cache.rate_limit_wait("fixture", now),
        Some(Duration::from_secs(120))
    );
    assert_eq!(peer.cache.rate_limit_wait("other", now), None);
    assert!(
        peer.cache.poll(now + Duration::from_secs(60)),
        "countdown redraws without a reply"
    );
    assert_eq!(
        peer.cache
            .rate_limit_wait("fixture", now + Duration::from_secs(60)),
        Some(Duration::from_secs(60))
    );
    assert!(peer.cache.poll(now + Duration::from_secs(120)));
    assert_eq!(
        peer.cache
            .rate_limit_wait("fixture", now + Duration::from_secs(120)),
        None
    );
}

#[test]
fn permission_pause_is_not_a_rate_limit_and_signout_clears_the_dot() {
    let mut peer = Peer::new();
    let now = Instant::now();
    peer.cache.store.borrow_mut().set_pause(
        "fixture",
        now + ERROR_BACKOFF,
        Some(&Error::Forbidden),
    );
    assert_eq!(peer.cache.rate_limit_wait("fixture", now), None);
    peer.cache.seed_rate_limit("fixture", now + ERROR_BACKOFF);
    assert!(peer.cache.rate_limit_wait("fixture", now).is_some());
    peer.cache.clear();
    assert_eq!(peer.cache.rate_limit_wait("fixture", now), None);
}
