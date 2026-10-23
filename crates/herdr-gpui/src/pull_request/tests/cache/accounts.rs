use super::*;
use crate::github::{Profile, Tokens};

/// `fixture` signed in as the main account and `emu` beside it, reading
/// the owners `owners` lists.
fn both(owners: &[String]) -> Tokens {
    let main = Profile::fixture("fixture", Arc::new("fixture".into()), &[]);
    let emu = Profile::fixture("emu", Arc::new("emu".into()), &[]);
    Tokens::new(&main, [(&emu, owners)])
}

/// A peer over both accounts, asking for one checkout at a time.
fn peer(owners: &[String]) -> Peer {
    let mut peer = Peer::new();
    let tokens = both(owners);
    let key = tokens.key();
    peer.cache
        .scope((0, 1, "boot".into()), tokens, &key, Origin::Local);
    peer
}

#[test]
fn a_paused_account_holds_only_its_own_checkouts() {
    let mut peer = peer(&[]);
    let now = Instant::now();
    // The extra account hit its rate limit; the main account did not.
    peer.cache.store.borrow_mut().set_pause(
        "emu",
        now + Duration::from_secs(600),
        Some("GitHub rate limit reached.".into()),
    );
    peer.cache.schedule([input("main-branch")], now);
    peer.cache.poll(now);
    let (generation, inputs, _, _, paused) = peer.incoming.try_recv().unwrap();
    assert_eq!(
        inputs,
        [input("main-branch")],
        "the main account still asks"
    );
    assert_eq!(paused, ["emu"], "the worker holds what emu reads");
    assert!(peer.cache.loading(&input("main-branch"), now));
    // The worker found the checkout is emu's and held it unasked.
    peer.outgoing
        .send((
            generation,
            Batch {
                held: vec![(input("main-branch"), "emu".into())],
                ..Batch::default()
            },
        ))
        .unwrap();
    peer.cache.poll(now);
    let mut view = Lookup::default();
    peer.cache.present(&input("main-branch"), &mut view, now);
    let message = view.message.unwrap();
    assert!(
        message.starts_with("GitHub rate limit reached. GitHub requests paused"),
        "{message}"
    );
    // Held until emu resumes, then asked again.
    peer.cache.schedule([input("main-branch")], now);
    assert!(peer.cache.queue.is_empty());
    let resumed = now + Duration::from_secs(600);
    peer.cache.schedule([input("main-branch")], resumed);
    peer.cache.poll(resumed);
    let (_, inputs, _, _, paused) = peer.incoming.try_recv().unwrap();
    assert_eq!(inputs, [input("main-branch")]);
    assert!(paused.is_empty());
}

#[test]
fn a_pause_is_kept_under_the_account_that_hit_it() {
    let mut peer = peer(&[]);
    let now = Instant::now();
    peer.cache.schedule([input("a"), input("b")], now);
    peer.cache.poll(now);
    let (generation, inputs, _, _, _) = peer.incoming.try_recv().unwrap();
    peer.outgoing
        .send((
            generation,
            Batch {
                found: vec![
                    (inputs[0].clone(), Ok(None)),
                    (
                        inputs[0].clone(),
                        Err(Error::GitHub(crate::github::Error::RateLimit {
                            reset: None,
                        })),
                    ),
                ],
                paused: vec![Paused {
                    login: "emu".into(),
                    cooldown: Duration::from_secs(600),
                    inputs: inputs.clone(),
                }],
                ..Batch::default()
            },
        ))
        .unwrap();
    peer.cache.poll(now);
    let store = peer.cache.store.borrow();
    assert!(store.pause("fixture", now).is_none(), "main keeps asking");
    let (until, reason) = store.pause("emu", now).unwrap();
    assert_eq!(until, now + Duration::from_secs(600));
    assert!(reason.unwrap().starts_with("GitHub rate limit"));
}

#[test]
fn every_account_paused_asks_nothing_and_says_so() {
    let mut peer = peer(&[]);
    let now = Instant::now();
    {
        let mut store = peer.cache.store.borrow_mut();
        store.set_pause("fixture", now + Duration::from_secs(900), None);
        store.set_pause("emu", now + Duration::from_secs(300), None);
    }
    let waiting = input("waiting");
    peer.cache.schedule([waiting.clone()], now);
    peer.cache.poll(now);
    assert!(peer.incoming.try_recv().is_err());
    assert!(!peer.cache.loading(&waiting, now));
    let mut view = Lookup::default();
    peer.cache.present(&waiting, &mut view, now);
    assert!(view.message.unwrap().contains("paused"));
    // The first account to resume asks again.
    let resumed = now + Duration::from_secs(300);
    peer.cache.poll(resumed);
    let (_, _, _, _, paused) = peer.incoming.try_recv().unwrap();
    assert_eq!(paused, ["fixture"]);
}

#[test]
fn moving_an_owner_to_an_account_does_not_reuse_what_the_old_one_hid() {
    let mut peer = peer(&[]);
    let now = Instant::now();
    let hidden = input("feature");
    peer.cache.refresh(hidden.clone(), now);
    peer.cache.poll(now);
    peer.complete(
        now,
        Err(Error::GitHub(crate::github::Error::RepositoryHidden {
            repository: "acme/project".into(),
            app_token: false,
        })),
        None,
    );
    peer.cache.schedule([hidden.clone()], now);
    assert!(peer.cache.queue.is_empty(), "hidden for the old set");
    // `acme` is now read by emu, which can see it.
    let tokens = both(&["acme".to_owned()]);
    let key = tokens.key();
    peer.cache
        .scope((0, 1, "boot".into()), tokens, &key, Origin::Local);
    assert!(peer.cache.lookup(&hidden).is_none());
    peer.cache.schedule([hidden.clone()], now);
    peer.cache.poll(now);
    let (_, inputs, _, _, _) = peer.incoming.try_recv().unwrap();
    assert_eq!(inputs, [hidden]);
}

#[test]
fn signing_out_forgets_every_accounts_pause() {
    let mut peer = peer(&[]);
    let now = Instant::now();
    peer.cache
        .store
        .borrow_mut()
        .set_pause("emu", now + ERROR_BACKOFF, None);
    peer.cache.clear();
    assert!(peer.cache.store.borrow().pauses.is_empty());
}
