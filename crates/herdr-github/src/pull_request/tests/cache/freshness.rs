use super::*;

fn with(state: State, running: bool) -> crate::pull_request::PullRequest {
    let mut pr = fixture().unwrap();
    pr.state = state;
    if let Some(checks) = pr.status_check_rollup.as_mut()
        && !running
    {
        checks.retain(|check| check.outcome() != Outcome::Pending);
    }
    pr
}

/// When `input`, answered with `value` at `now`, is asked about again.
fn next_ask(peer: &mut Peer, input: &Input, now: Instant, value: Result) -> Duration {
    peer.cache.schedule([input.clone()], now);
    peer.cache.poll(now);
    peer.complete(now, value, None);
    let store = peer.cache.store.borrow();
    let entry = store
        .entries
        .iter()
        .find(|entry| &entry.input == input)
        .unwrap();
    entry.due - now
}

#[test]
fn the_less_a_pull_request_can_change_the_less_it_is_asked_about() {
    let mut peer = Peer::new();
    let now = Instant::now();
    for (branch, value, wait) in [
        ("running", Ok(Some(with(State::Open, true))), REFRESH),
        ("quiet", Ok(Some(with(State::Open, false))), QUIET_REFRESH),
        ("none", Ok(None), QUIET_REFRESH),
        (
            "merged",
            Ok(Some(with(State::Merged, false))),
            SETTLED_REFRESH,
        ),
        (
            "closed",
            Ok(Some(with(State::Closed, true))),
            SETTLED_REFRESH,
        ),
        (
            "failed",
            Err(crate::pull_request::Error::Origin.into()),
            ERROR_BACKOFF,
        ),
    ] {
        assert_eq!(
            next_ask(&mut peer, &input(branch), now, value),
            wait,
            "{branch}"
        );
    }
}

#[test]
fn the_viewed_workspace_asks_sooner_unless_its_pull_request_is_settled() {
    let mut peer = Peer::new();
    let now = Instant::now();
    let open = input("open");
    let merged = input("merged");
    next_ask(&mut peer, &open, now, Ok(Some(with(State::Open, false))));
    next_ask(
        &mut peer,
        &merged,
        now,
        Ok(Some(with(State::Merged, false))),
    );
    let viewed = now + REFRESH;
    for input in [&open, &merged] {
        peer.cache.expire_viewed(input, viewed);
    }
    peer.cache.schedule([open.clone(), merged.clone()], viewed);
    assert_eq!(peer.cache.queue, [open]);
}

#[test]
fn an_idle_window_waits_longer_but_an_explicit_refresh_does_not() {
    let mut peer = Peer::new();
    let now = Instant::now();
    let quiet = input("quiet");
    let asked = input("asked");
    for input in [&quiet, &asked] {
        next_ask(&mut peer, input, now, Ok(Some(with(State::Open, false))));
    }
    peer.cache.idle = true;
    let later = now + QUIET_REFRESH;
    peer.cache.schedule([quiet.clone(), asked.clone()], later);
    assert!(peer.cache.queue.is_empty(), "idle stretches the wait");
    peer.cache.refresh(asked.clone(), later);
    peer.cache.poll(later);
    assert_eq!(peer.complete(later, Ok(None), None), asked);
    let stretched = now + QUIET_REFRESH * IDLE_STRETCH;
    peer.cache.schedule([quiet.clone()], stretched);
    assert_eq!(peer.cache.queue, std::slice::from_ref(&quiet));
    peer.cache.idle = false;
    peer.cache.schedule([quiet.clone()], later);
    assert_eq!(peer.cache.queue, [quiet], "back in front, the normal wait");
}

#[test]
fn an_idle_window_stretches_the_viewed_workspace_too() {
    let mut peer = Peer::new();
    let now = Instant::now();
    let open = input("open");
    next_ask(&mut peer, &open, now, Ok(Some(with(State::Open, false))));
    peer.cache.idle = true;
    let viewed = now + REFRESH;
    peer.cache.expire_viewed(&open, viewed);
    peer.cache.schedule([open.clone()], viewed);
    assert!(peer.cache.queue.is_empty(), "nobody is looking");
    let stretched = now + REFRESH * IDLE_STRETCH;
    peer.cache.schedule([open.clone()], stretched);
    assert_eq!(peer.cache.queue, std::slice::from_ref(&open));
}
