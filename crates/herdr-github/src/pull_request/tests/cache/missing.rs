use super::*;

#[test]
fn a_new_pr_is_discovered_early_only_for_the_workspace_being_viewed() {
    let mut peer = Peer::new();
    let now = Instant::now();
    let focused = input("focused");
    let background = input("background");
    let inputs = [focused.clone(), background.clone()];
    peer.cache.schedule(inputs.clone(), now);
    peer.cache.poll(now);
    peer.complete(now, Ok(None), None);
    peer.complete(now, Ok(None), None);

    let before = now + Duration::from_secs(14);
    peer.cache.expire_viewed(&focused, before);
    peer.cache.schedule(inputs.clone(), before);
    peer.cache.poll(before);
    assert!(peer.incoming.try_recv().is_err());

    let due = now + Duration::from_secs(15);
    peer.cache.expire_viewed(&focused, due);
    assert!(
        peer.incoming.try_recv().is_err(),
        "expiry only changes metadata"
    );
    peer.cache.schedule(inputs.clone(), due);
    peer.cache.poll(due);
    assert_eq!(
        peer.complete(due, Ok(Some(fixture().unwrap())), None),
        focused
    );
    let mut view = Lookup::default();
    peer.cache.present(&focused, &mut view, due);
    assert_eq!(view.value.as_ref().unwrap().number, 8);
    assert!(
        peer.incoming.try_recv().is_err(),
        "background keeps its TTL"
    );

    peer.cache
        .expire_viewed(&focused, due + Duration::from_secs(15));
    peer.cache.schedule(inputs, due + Duration::from_secs(15));
    peer.cache.poll(due + Duration::from_secs(15));
    assert!(
        peer.incoming.try_recv().is_err(),
        "found PR keeps its normal TTL"
    );
    // Nothing to show is quiet: it waits longer than a pull request with
    // checks running.
    peer.cache.schedule([background.clone()], now + REFRESH);
    assert!(peer.cache.queue.is_empty());
    peer.cache
        .schedule([background.clone()], now + QUIET_REFRESH);
    peer.cache.poll(now + QUIET_REFRESH);
    assert_eq!(
        peer.complete(now + QUIET_REFRESH, Ok(None), None),
        background
    );
}

#[test]
fn repeated_empty_results_are_spaced_and_in_flight_work_is_not_duplicated() {
    let mut peer = Peer::new();
    let now = Instant::now();
    let input = input("focused");
    peer.cache.schedule([input.clone()], now);
    peer.cache.poll(now);
    peer.complete(now, Ok(None), None);
    let due = now + Duration::from_secs(15);
    peer.cache.expire_viewed(&input, due);
    peer.cache.schedule([input.clone()], due);
    peer.cache.poll(due);
    peer.cache.expire_viewed(&input, due);
    peer.cache.schedule([input.clone()], due);
    assert!(peer.cache.queue.is_empty());
    peer.complete(due, Ok(None), None);

    let before = due + Duration::from_secs(14);
    peer.cache.expire_viewed(&input, before);
    peer.cache.schedule([input.clone()], before);
    peer.cache.poll(before);
    assert!(peer.incoming.try_recv().is_err());
    let next = due + Duration::from_secs(15);
    peer.cache.expire_viewed(&input, next);
    peer.cache.schedule([input.clone()], next);
    peer.cache.poll(next);
    assert_eq!(peer.complete(next, Ok(None), None), input);
}

#[test]
fn early_missing_refresh_preserves_error_and_account_backoff() {
    let mut peer = Peer::new();
    let now = Instant::now();
    let failed = input("failed");
    let missing = input("missing");
    peer.cache.schedule([failed.clone(), missing.clone()], now);
    peer.cache.poll(now);
    peer.complete(now, Err(crate::pull_request::Error::Origin.into()), None);
    peer.complete(now, Ok(None), None);
    let due = now + Duration::from_secs(15);
    peer.cache.expire_viewed(&failed, due);
    peer.cache.schedule([failed.clone()], due);
    peer.cache.poll(due);
    assert!(peer.incoming.try_recv().is_err());

    peer.pause(now + ERROR_BACKOFF);
    peer.cache.expire_viewed(&missing, due);
    peer.cache.schedule([missing.clone()], due);
    peer.cache.poll(due);
    assert!(peer.incoming.try_recv().is_err());
    peer.cache
        .schedule([failed.clone(), missing.clone()], now + ERROR_BACKOFF);
    peer.cache.poll(now + ERROR_BACKOFF);
    assert_eq!(peer.complete(now + ERROR_BACKOFF, Ok(None), None), failed);
    assert_eq!(peer.complete(now + ERROR_BACKOFF, Ok(None), None), missing);
}
