use super::*;

#[test]
fn due_checkouts_share_one_request_and_each_keeps_its_own_answer() {
    let mut peer = Peer::batched(BATCH_LIMIT);
    let now = Instant::now();
    peer.cache
        .schedule((0..BATCH_LIMIT + 5).map(|i| input(&i.to_string())), now);
    peer.cache.poll(now);
    let (generation, inputs, _, _, _) = peer.incoming.try_recv().unwrap();
    assert_eq!(inputs.len(), BATCH_LIMIT);
    assert!(peer.cache.loading(&input("0"), now));
    let found = inputs
        .into_iter()
        .map(|input| {
            let result = match input.branch.as_str() {
                "0" => Ok(Some(fixture().unwrap())),
                "1" => Err(crate::pull_request::Error::Origin.into()),
                _ => Ok(None),
            };
            (input, result)
        })
        .collect();
    peer.outgoing
        .send((
            generation,
            Batch {
                found,
                ..Batch::default()
            },
        ))
        .unwrap();
    peer.cache.poll(now);
    assert_eq!(
        peer.cache.peek(&local(), "/repo/.git", "0").unwrap().number,
        8
    );
    let (value, message) = peer.cache.lookup(&input("1")).unwrap();
    assert!(
        value.is_none() && message.is_some(),
        "a local failure is its own"
    );
    assert!(peer.cache.lookup(&input("2")).unwrap().1.is_none());
    // The rest go together in the next request.
    let (_, rest, _, _, _) = peer.incoming.try_recv().unwrap();
    assert_eq!(rest.len(), 5);
}

#[test]
fn a_failed_request_keeps_every_checkouts_last_answer_and_pauses_once() {
    let mut peer = Peer::batched(BATCH_LIMIT);
    let now = Instant::now();
    for branch in ["first", "second"] {
        peer.cache.seed(input(branch), fixture().unwrap(), now);
        peer.cache.refresh(input(branch), now);
    }
    peer.cache.poll(now);
    let (generation, inputs, _, _, _) = peer.incoming.try_recv().unwrap();
    assert_eq!(inputs, [input("second"), input("first")]);
    peer.outgoing
        .send((
            generation,
            Batch {
                found: Vec::new(),
                failed: vec![(inputs.clone(), Error::RateLimit { reset: None })],
                paused: vec![Paused {
                    login: "fixture".into(),
                    cooldown: Duration::from_secs(600),
                    inputs,
                }],
                ..Batch::default()
            },
        ))
        .unwrap();
    peer.cache.poll(now);
    let mut view = Lookup::default();
    for branch in ["first", "second"] {
        peer.cache.present(&input(branch), &mut view, now);
        assert_eq!(view.value.as_ref().unwrap().number, 8);
        assert!(
            view.message
                .as_deref()
                .unwrap()
                .starts_with("Refresh unavailable, showing the last result. GitHub rate limit")
        );
    }
    let store = peer.cache.store.borrow();
    let (until, reason) = store.pause("fixture", now).unwrap();
    assert_eq!(until, now + Duration::from_secs(600));
    assert!(reason.unwrap().starts_with("GitHub rate limit"));
}

#[test]
fn a_checkout_gone_mid_request_is_dropped_but_the_rest_are_answered() {
    let mut peer = Peer::batched(BATCH_LIMIT);
    let now = Instant::now();
    peer.cache.schedule([input("kept"), input("gone")], now);
    peer.cache.poll(now);
    let (generation, inputs, _, _, _) = peer.incoming.try_recv().unwrap();
    peer.cache.retain(|input| input.branch == "kept");
    let found = inputs
        .into_iter()
        .map(|input| (input, Ok(Some(fixture().unwrap()))))
        .collect();
    peer.outgoing
        .send((
            generation,
            Batch {
                found,
                ..Batch::default()
            },
        ))
        .unwrap();
    peer.cache.poll(now);
    assert!(peer.cache.peek(&local(), "/repo/.git", "kept").is_some());
    assert!(peer.cache.lookup(&input("gone")).is_none());
}

#[test]
fn a_worker_that_stops_fails_its_whole_batch_and_keeps_answers() {
    let mut peer = Peer::batched(BATCH_LIMIT);
    let now = Instant::now();
    peer.cache.seed(input("first"), fixture().unwrap(), now);
    peer.cache.refresh(input("first"), now);
    peer.cache.refresh(input("second"), now);
    peer.cache.poll(now);
    drop(peer.incoming);
    drop(peer.outgoing);
    peer.cache.poll(now);
    assert!(peer.cache.active.is_empty());
    assert_eq!(
        peer.cache
            .peek(&local(), "/repo/.git", "first")
            .unwrap()
            .number,
        8
    );
    let (_, message) = peer.cache.lookup(&input("second")).unwrap();
    assert_eq!(
        message.as_deref(),
        Some("PR worker stopped; retrying automatically.")
    );
}
