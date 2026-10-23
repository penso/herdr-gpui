use super::*;

fn hidden() -> Error {
    Error::GitHub(crate::github::Error::RepositoryHidden {
        repository: "acme/hidden".into(),
        app_token: true,
    })
}

fn elsewhere(branch: &str) -> Input {
    Input {
        repo_key: Some("/other/.git".into()),
        ..input(branch)
    }
}

#[test]
fn a_hidden_repository_pauses_all_its_branches_but_not_other_repositories() {
    let mut peer = Peer::new();
    let now = Instant::now();
    peer.cache
        .schedule([input("first"), input("second"), elsewhere("third")], now);
    peer.cache.poll(now);
    assert_eq!(peer.complete(now, Err(hidden()), None).branch, "first");

    // The sibling branch is answered from the pause; the other repository asks.
    let (_, next, _, _, _) = peer.incoming.try_recv().unwrap();
    assert_eq!(next, [elsewhere("third")]);
    let mut view = Lookup::default();
    peer.cache.present(&input("second"), &mut view, now);
    assert!(
        view.message
            .clone()
            .unwrap()
            .starts_with("GitHub cannot see acme/hidden")
    );
    assert!(!view.loading);
}

#[test]
fn a_hidden_repository_asks_again_only_after_its_backoff() {
    let mut peer = Peer::new();
    let now = Instant::now();
    peer.cache.schedule([input("first"), input("second")], now);
    peer.cache.poll(now);
    peer.complete(now, Err(hidden()), None);
    assert!(peer.incoming.try_recv().is_err());

    let before = now + HIDDEN_BACKOFF - Duration::from_secs(1);
    peer.cache
        .schedule([input("first"), input("second")], before);
    peer.cache.poll(before);
    assert!(
        peer.incoming.try_recv().is_err(),
        "no branch asks while paused"
    );

    let due = now + HIDDEN_BACKOFF;
    peer.cache.schedule([input("first"), input("second")], due);
    peer.cache.poll(due);
    let (_, next, _, _, _) = peer.incoming.try_recv().unwrap();
    assert_eq!(next, [input("first")]);
}

#[test]
fn asking_explicitly_lifts_the_repository_pause() {
    let mut peer = Peer::new();
    let now = Instant::now();
    peer.cache.schedule([input("first")], now);
    peer.cache.poll(now);
    peer.complete(now, Err(hidden()), None);
    peer.cache.refresh(input("second"), now);
    peer.cache.poll(now);
    let (_, next, _, _, _) = peer.incoming.try_recv().unwrap();
    assert_eq!(next, [input("second")]);
}

#[test]
fn an_ordinary_failure_does_not_pause_the_repository() {
    let mut peer = Peer::new();
    let now = Instant::now();
    peer.cache.schedule([input("first"), input("second")], now);
    peer.cache.poll(now);
    peer.complete(now, Err(Error::GitHub(crate::github::Error::Query)), None);
    let (_, next, _, _, _) = peer.incoming.try_recv().unwrap();
    assert_eq!(next, [input("second")]);
}

#[test]
fn hidden_branches_never_grow_a_full_cache() {
    let mut peer = Peer::new();
    let now = Instant::now();
    peer.cache.schedule([input("first")], now);
    peer.cache.poll(now);
    peer.complete(now, Err(hidden()), None);
    peer.cache
        .schedule((1..CACHE_LIMIT).map(|i| elsewhere(&i.to_string())), now);
    peer.cache.poll(now);
    for _ in 1..CACHE_LIMIT {
        peer.complete(now, Ok(None), None);
    }
    assert_eq!(peer.cache.store.borrow().entries.len(), CACHE_LIMIT);
    assert!(
        peer.cache
            .store
            .borrow()
            .entries
            .iter()
            .all(|entry| now < entry.due)
    );

    // Every sibling is answered from the pause while nothing in the cache is due.
    peer.cache
        .schedule((0..8).map(|i| input(&format!("sibling-{i}"))), now);
    peer.cache.poll(now);
    assert!(peer.incoming.try_recv().is_err());
    assert_eq!(peer.cache.store.borrow().entries.len(), CACHE_LIMIT);
}
