use super::*;

/// A second window's cache over the same store, with its own worker.
fn sibling(peer: &Peer) -> Peer {
    let mut other = Peer::new();
    other.cache.store = peer.cache.store.clone();
    other
}

#[test]
fn windows_on_the_same_checkouts_ask_github_once_and_share_the_answer() {
    let mut first = Peer::new();
    let mut second = sibling(&first);
    let now = Instant::now();
    let feature = input("feature");
    first.cache.schedule([feature.clone()], now);
    first.cache.poll(now);
    second.cache.schedule([feature.clone()], now);
    second.cache.poll(now);
    assert!(second.incoming.try_recv().is_err(), "already being asked");
    assert!(second.cache.loading(&feature, now));
    first.complete(now, Ok(Some(fixture().unwrap())), None);
    assert_eq!(
        second
            .cache
            .peek(&local(), "/repo/.git", "feature")
            .unwrap()
            .number,
        8
    );
    second.cache.schedule([feature.clone()], now);
    assert!(second.cache.queue.is_empty(), "fresh for every window");
}

#[test]
fn a_rate_limit_one_window_hit_pauses_the_others() {
    let mut first = Peer::new();
    let mut second = sibling(&first);
    let now = Instant::now();
    first.cache.schedule([input("limited")], now);
    first.cache.poll(now);
    first.complete(
        now,
        Err(Error::RateLimit { reset: None }),
        Some(Duration::from_secs(600)),
    );
    second.cache.schedule([input("other")], now);
    second.cache.poll(now);
    assert!(second.incoming.try_recv().is_err());
    second.cache.poll(now + Duration::from_secs(600));
    assert_eq!(second.incoming.try_recv().unwrap().1, [input("other")]);
}

#[test]
fn a_closed_window_releases_what_it_was_asking() {
    let mut first = Peer::new();
    let mut second = sibling(&first);
    let now = Instant::now();
    first.cache.schedule([input("feature")], now);
    first.cache.poll(now);
    drop(first);
    second.cache.schedule([input("feature")], now);
    second.cache.poll(now);
    assert_eq!(second.incoming.try_recv().unwrap().1, [input("feature")]);
}

#[test]
fn signing_out_forgets_only_that_accounts_answers() {
    let mut first = Peer::new();
    let mut second = sibling(&first);
    let now = Instant::now();
    first.cache.seed(input("mine"), fixture().unwrap(), now);
    let token = second.cache.token.clone().unwrap();
    second
        .cache
        .scope((0, 1, "boot".into()), token, "someone-else", Origin::Local);
    second.cache.seed(input("theirs"), fixture().unwrap(), now);
    first.cache.clear();
    assert!(
        second
            .cache
            .peek(&second.cache.view, "/repo/.git", "theirs")
            .is_some()
    );
    assert!(
        first
            .cache
            .store
            .borrow()
            .entries
            .iter()
            .all(|entry| entry.view.login == "someone-else")
    );
}

#[test]
fn a_window_redraws_when_another_records_an_answer() {
    let mut first = Peer::new();
    let mut second = sibling(&first);
    let now = Instant::now();
    second.cache.poll(now);
    assert!(!second.cache.poll(now), "nothing new");
    first.cache.schedule([input("feature")], now);
    first.cache.poll(now);
    assert!(second.cache.poll(now), "the other window is now asking");
    first.complete(now, Ok(Some(fixture().unwrap())), None);
    assert!(second.cache.poll(now), "and has an answer to show");
    assert!(!second.cache.poll(now));
}

#[test]
fn a_disconnected_or_signed_out_window_does_not_redraw_the_others() {
    let mut first = Peer::new();
    let mut second = sibling(&first);
    let now = Instant::now();
    second.cache.poll(now);
    for _ in 0..3 {
        first.cache.suspend();
        first.cache.clear();
    }
    assert!(
        !second.cache.poll(now),
        "nothing the other window shows changed"
    );
}
