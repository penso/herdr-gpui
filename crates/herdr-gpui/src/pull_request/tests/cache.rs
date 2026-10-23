use super::*;
use crate::pull_request::{
    batch::{BATCH_LIMIT, Batch, Paused},
    cache::{
        BatchSize, CACHE_LIMIT, ERROR_BACKOFF, HIDDEN_BACKOFF, IDLE_STRETCH, QUIET_REFRESH,
        REFRESH, SETTLED_REFRESH,
    },
};

mod accounts;
mod batched;
mod freshness;
mod hidden;
mod missing;
mod pause;
mod shared;

/// The view a [`Peer`] starts in: this machine, read with the fixture account.
fn local() -> crate::pull_request::View {
    crate::pull_request::View {
        origin: Origin::Local,
        login: "fixture".into(),
    }
}

fn input(branch: &str) -> Input {
    Input {
        checkout: None,
        repo_key: Some("/repo/.git".into()),
        branch: branch.into(),
    }
}

struct Peer {
    cache: Cache,
    incoming: mpsc::Receiver<crate::pull_request::lookup::Request>,
    outgoing: mpsc::SyncSender<(u64, Batch)>,
}

/// The fixture account alone, signed in as `fixture`.
fn tokens() -> crate::github::Tokens {
    let profile = crate::github::Profile::fixture("fixture", Arc::new("fixture".into()), &[]);
    crate::github::Tokens::new(&profile, [])
}

fn answered(input: Input, result: Result) -> Batch {
    Batch {
        found: vec![(input, result)],
        ..Batch::default()
    }
}

impl Peer {
    /// One checkout per request, as most tests follow lookups.
    fn new() -> Self {
        Self::batched(1)
    }

    fn batched(size: usize) -> Self {
        let (requests, incoming) = mpsc::sync_channel(1);
        let (outgoing, results) = mpsc::sync_channel(1);
        let mut cache = Cache::default();
        cache.prefetch.worker = Some(Worker { requests, results });
        cache.batch = BatchSize(size);
        cache.scope((0, 1, "boot".into()), tokens(), "fixture", Origin::Local);
        Self {
            cache,
            incoming,
            outgoing,
        }
    }

    fn pause(&self, until: Instant) {
        self.cache
            .store
            .borrow_mut()
            .set_pause("fixture", until, None);
    }

    fn complete(&mut self, now: Instant, result: Result, cooldown: Option<Duration>) -> Input {
        let (generation, mut inputs, _, _, _) = self.incoming.try_recv().unwrap();
        assert_eq!(inputs.len(), 1);
        let input = inputs.pop().unwrap();
        let mut batch = answered(input.clone(), result);
        batch.paused.extend(cooldown.map(|cooldown| Paused {
            login: "fixture".into(),
            cooldown,
            inputs: vec![input.clone()],
        }));
        self.outgoing.send((generation, batch)).unwrap();
        self.cache.poll(now);
        input
    }
}

#[test]
fn explicit_refresh_keeps_cached_details_and_coalesces_in_flight_requests() {
    let mut peer = Peer::new();
    let now = Instant::now();
    let input = input("feature");
    peer.cache.seed(input.clone(), fixture().unwrap(), now);
    peer.cache.refresh(input.clone(), now);
    peer.cache.refresh(input.clone(), now);
    assert_eq!(peer.cache.queue.len(), 1);
    assert!(!peer.cache.scan_due(now));
    assert!(peer.incoming.try_recv().is_err(), "input only queues work");
    peer.cache.poll(now);
    assert_eq!(
        peer.cache
            .peek(&local(), input.repo_key.as_deref().unwrap(), &input.branch)
            .unwrap()
            .number,
        8
    );
    peer.cache.refresh(input.clone(), now);
    assert!(peer.cache.queue.is_empty(), "reuse the in-flight lookup");
    let mut updated = fixture().unwrap();
    updated.is_draft = true;
    updated.checks_summary = "2 pending".into();
    peer.complete(now, Ok(Some(updated)), None);
    let pr = peer
        .cache
        .peek(&local(), input.repo_key.as_deref().unwrap(), &input.branch)
        .unwrap();
    assert!(pr.is_draft);
    assert_eq!(pr.checks_summary, "2 pending");
    assert!(peer.incoming.try_recv().is_err());
}

#[test]
fn explicit_refresh_respects_account_backoff_and_bounds_the_queue() {
    let mut peer = Peer::new();
    let now = Instant::now();
    let input = input("feature");
    peer.cache.seed(input.clone(), fixture().unwrap(), now);
    peer.pause(now + ERROR_BACKOFF);
    peer.cache
        .schedule((0..CACHE_LIMIT).map(|i| self::input(&i.to_string())), now);
    peer.cache.refresh(input.clone(), now);
    assert_eq!(peer.cache.queue.len(), CACHE_LIMIT);
    assert_eq!(peer.cache.queue.front(), Some(&input));
    peer.cache.poll(now);
    assert!(peer.incoming.try_recv().is_err());
    peer.cache.poll(now + ERROR_BACKOFF);
    assert_eq!(peer.complete(now + ERROR_BACKOFF, Ok(None), None), input);
    assert!(
        peer.cache
            .peek(&local(), input.repo_key.as_deref().unwrap(), &input.branch)
            .is_none()
    );
}

#[test]
fn loading_covers_queued_and_in_flight_lookups_but_not_a_paused_account() {
    let mut peer = Peer::new();
    let now = Instant::now();
    let input = input("feature");
    assert!(!peer.cache.loading(&input, now));
    peer.cache.refresh(input.clone(), now);
    assert!(peer.cache.loading(&input, now), "queued for dispatch");
    peer.pause(now + ERROR_BACKOFF);
    assert!(!peer.cache.loading(&input, now), "a paused account waits");
    peer.cache.store.borrow_mut().pauses.clear();
    peer.cache.poll(now);
    assert!(peer.cache.loading(&input, now), "in flight");
    assert!(!peer.cache.loading(&self::input("other"), now));
    peer.complete(now, Ok(Some(fixture().unwrap())), None);
    assert!(!peer.cache.loading(&input, now));
}

#[test]
fn cache_prefetches_without_menu_and_refreshes_at_ttl_with_stale_data() {
    let mut peer = Peer::new();
    let now = Instant::now();
    let input = input("feature");
    peer.cache.schedule([input.clone(), input.clone()], now);
    assert_eq!(peer.cache.queue.len(), 1);
    peer.cache.poll(now);
    assert_eq!(
        peer.complete(now, Ok(Some(fixture().unwrap())), None),
        input
    );
    let mut view = Lookup::default();
    peer.cache.present(&input, &mut view, now);
    assert_eq!(view.value.as_ref().unwrap().number, 8);
    assert!(!view.loading);
    assert!(peer.incoming.try_recv().is_err(), "menu read is I/O free");
    // A check is still running, so it is asked about again soon.
    peer.cache
        .schedule([input.clone()], now + REFRESH - Duration::from_secs(1));
    peer.cache.poll(now + REFRESH - Duration::from_secs(1));
    assert!(peer.incoming.try_recv().is_err());
    let due = now + REFRESH;
    peer.cache.schedule([input.clone()], due);
    peer.cache.poll(due);
    peer.cache.present(&input, &mut view, due);
    assert!(
        view.value.is_some(),
        "refresh does not replace cached data with loading"
    );
    peer.complete(
        due,
        Err(std::io::Error::other("network unavailable").into()),
        None,
    );
    peer.cache.present(&input, &mut view, due);
    assert!(view.value.is_some());
    assert!(view.message.is_some());
    peer.cache.schedule(
        [input.clone()],
        due + ERROR_BACKOFF - Duration::from_secs(1),
    );
    assert!(peer.cache.queue.is_empty());
    peer.cache.schedule([input.clone()], due + ERROR_BACKOFF);
    peer.cache.poll(due + ERROR_BACKOFF);
    peer.complete(due + ERROR_BACKOFF, Ok(None), None);
    peer.cache.present(&input, &mut view, due + ERROR_BACKOFF);
    assert!(view.value.is_none() && view.message.is_none() && !view.loading);
    peer.cache.schedule([input], due + ERROR_BACKOFF);
    assert!(
        peer.cache.queue.is_empty(),
        "negative results also have a TTL"
    );
}

#[test]
fn scope_changes_fence_late_results_and_only_another_account_forgets_answers() {
    let now = Instant::now();
    let scope = |epoch, generation, boot: &str| (epoch, generation, boot.to_owned());
    for change in 0..8 {
        let mut peer = Peer::new();
        peer.cache.seed(input("cached"), fixture().unwrap(), now);
        peer.cache.schedule([input("old")], now);
        peer.cache.poll(now);
        let (generation, _, _, _, _) = peer.incoming.try_recv().unwrap();
        let token = peer.cache.token.as_ref().unwrap().clone();
        match change {
            0 => peer.cache.clear(), // sign-out
            1 => peer.cache.scope(
                scope(0, 1, "boot"),
                Arc::new("other".into()),
                "other-account",
                Origin::Local,
            ),
            2 => peer.cache.scope(
                scope(0, 1, "boot"),
                Arc::new("renewed".into()),
                "fixture",
                Origin::Local,
            ),
            3 => peer
                .cache
                .scope(scope(1, 1, "boot"), token, "fixture", Origin::Local),
            4 => peer
                .cache
                .scope(scope(0, 2, "boot"), token, "fixture", Origin::Local),
            5 => peer
                .cache
                .scope(scope(0, 1, "new-boot"), token, "fixture", Origin::Local),
            6 => peer.cache.scope(
                scope(0, 1, "boot"),
                token,
                "fixture",
                Origin::Ssh("host".into()),
            ),
            _ => peer.cache.retain(|input| input.branch == "new"),
        }
        assert_eq!(
            peer.cache
                .peek(&peer.cache.view, "/repo/.git", "cached")
                .is_some(),
            (2..=5).contains(&change),
            "cached answer after change {change}"
        );
        // Another account or device keeps this one's answers for its rows;
        // only signing out or the checkout going away drops them.
        assert_eq!(
            peer.cache.peek(&local(), "/repo/.git", "cached").is_some(),
            (1..=6).contains(&change),
            "kept answer after change {change}"
        );
        assert!(peer.cache.queue.is_empty());
        peer.outgoing
            .send((
                generation,
                answered(input("old"), Ok(Some(fixture().unwrap()))),
            ))
            .unwrap();
        peer.cache.poll(now);
        assert!(
            peer.cache
                .store
                .borrow()
                .entries
                .iter()
                .all(|e| e.input != input("old")),
            "late result was recorded: {change}"
        );
        assert!(peer.incoming.try_recv().is_err());
    }
}

#[test]
fn another_device_keeps_this_ones_answers_for_when_it_is_selected_again() {
    let mut peer = Peer::new();
    let now = Instant::now();
    peer.cache.seed(input("feature"), fixture().unwrap(), now);
    let token = peer.cache.token.clone().unwrap();
    let remote = Origin::Ssh("host".into());
    peer.cache.scope(
        (1, 1, "boot".into()),
        token.clone(),
        "fixture",
        remote.clone(),
    );
    assert!(
        peer.cache
            .peek(
                &crate::pull_request::View {
                    origin: remote.clone(),
                    login: "fixture".into(),
                },
                "/repo/.git",
                "feature",
            )
            .is_none()
    );
    // The same path on that device is another checkout: it is asked for.
    peer.cache.schedule([input("feature")], now);
    assert_eq!(peer.cache.queue.len(), 1);
    // Its checkouts going away does not drop this device's entries.
    peer.cache.retain(|_| false);
    peer.cache
        .scope((2, 1, "boot".into()), token, "fixture", Origin::Local);
    assert_eq!(
        peer.cache
            .peek(&local(), "/repo/.git", "feature")
            .unwrap()
            .number,
        8
    );
}

#[test]
fn a_dropped_connection_keeps_answers_and_the_account_pause() {
    let mut peer = Peer::new();
    let now = Instant::now();
    let feature = input("feature");
    peer.cache.seed(feature.clone(), fixture().unwrap(), now);
    peer.cache.refresh(feature.clone(), now);
    peer.cache.poll(now);
    peer.complete(
        now,
        Err(Error::GitHub(crate::github::Error::RateLimit {
            reset: None,
        })),
        Some(Duration::from_secs(600)),
    );
    peer.cache.suspend();
    let token = peer.cache.token.clone().unwrap();
    peer.cache
        .scope((0, 2, "boot".into()), token, "fixture", Origin::Local);
    let mut view = Lookup::default();
    peer.cache.present(&feature, &mut view, now);
    assert_eq!(view.value.as_ref().unwrap().number, 8);
    assert!(
        view.message
            .unwrap()
            .starts_with("Refresh unavailable, showing the last result. GitHub rate limit")
    );
    // The reconnect does not ask a rate-limited account again.
    peer.cache.schedule([input("other")], now);
    peer.cache.poll(now);
    assert!(peer.incoming.try_recv().is_err());
}

#[test]
fn signout_drains_private_results_without_starting_queued_work() {
    let mut peer = Peer::new();
    let now = Instant::now();
    peer.cache.schedule([input("active"), input("queued")], now);
    peer.cache.poll(now);
    let (generation, _, _, _, _) = peer.incoming.try_recv().unwrap();
    peer.outgoing
        .send((
            generation,
            answered(input("active"), Ok(Some(fixture().unwrap()))),
        ))
        .unwrap();
    peer.cache.clear();
    assert!(!peer.cache.prefetch.busy);
    assert!(
        peer.cache
            .prefetch
            .worker
            .as_ref()
            .unwrap()
            .results
            .try_recv()
            .is_err()
    );
    assert!(peer.incoming.try_recv().is_err());
    assert!(peer.cache.token.is_none());
}

#[test]
fn cache_is_bounded_lru_and_does_not_refetch_fresh_entries_under_pressure() {
    let mut peer = Peer::new();
    let now = Instant::now();
    peer.cache
        .schedule((0..CACHE_LIMIT * 2).map(|i| input(&i.to_string())), now);
    assert_eq!(peer.cache.queue.len(), CACHE_LIMIT);
    peer.cache.poll(now);
    for _ in 0..CACHE_LIMIT {
        peer.complete(now, Ok(None), None);
    }
    assert_eq!(peer.cache.store.borrow().entries.len(), CACHE_LIMIT);
    assert!(peer.incoming.try_recv().is_err());
    peer.cache.schedule([input("overflow")], now);
    peer.cache.poll(now);
    assert!(
        peer.incoming.try_recv().is_err(),
        "do not evict fresh data to hammer GitHub"
    );
    let mut view = Lookup::default();
    peer.cache
        .present(&input("0"), &mut view, now + Duration::from_secs(1));
    peer.cache
        .schedule([input("overflow")], now + QUIET_REFRESH);
    peer.cache.poll(now + QUIET_REFRESH);
    peer.complete(now + QUIET_REFRESH, Ok(None), None);
    assert_eq!(peer.cache.store.borrow().entries.len(), CACHE_LIMIT);
    assert!(
        peer.cache
            .store
            .borrow()
            .entries
            .iter()
            .any(|e| e.input.branch == "0")
    );
    assert!(
        !peer
            .cache
            .store
            .borrow()
            .entries
            .iter()
            .any(|e| e.input.branch == "1")
    );
    assert!(
        peer.cache
            .store
            .borrow()
            .entries
            .iter()
            .any(|e| e.input.branch == "overflow")
    );
}

#[test]
fn local_failures_do_not_starve_other_repos_but_rate_limits_pause_account() {
    let mut peer = Peer::new();
    let now = Instant::now();
    peer.cache.schedule(
        [input("local-error"), input("limited"), input("waiting")],
        now,
    );
    peer.cache.poll(now);
    assert_eq!(
        peer.complete(now, Err(crate::pull_request::Error::Origin.into()), None)
            .branch,
        "local-error"
    );
    assert_eq!(
        peer.complete(
            now,
            Err(Error::GitHub(crate::github::Error::RateLimit {
                reset: None
            })),
            Some(Duration::from_secs(3600))
        )
        .branch,
        "limited"
    );
    peer.cache.poll(now + Duration::from_secs(3599));
    assert!(peer.incoming.try_recv().is_err());
    let mut view = Lookup::default();
    peer.cache.present(&input("waiting"), &mut view, now);
    assert!(view.message.as_deref().unwrap().contains("paused"));
    peer.cache.poll(now + Duration::from_secs(3600));
    assert_eq!(
        peer.complete(now + Duration::from_secs(3600), Ok(None), None)
            .branch,
        "waiting"
    );
}
