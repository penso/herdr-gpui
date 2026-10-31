use super::*;

#[test]
fn publication_is_lazy_and_a_busy_stream_cannot_postpone_the_deadline() {
    let mut streams = Streams::default();
    let now = Instant::now();
    assert!(streams.insert("command", String::new()));
    assert!(streams.next_tick().is_none());
    let mut publications = 0;
    for (index, character) in "hello".chars().enumerate() {
        let result = streams.update(
            "command",
            now + Duration::from_millis(index as u64),
            |text| {
                text.push(character);
                true
            },
            |_, text| {
                publications += 1;
                text.clone()
            },
        );
        assert_eq!(result, (index == 0).then(|| "h".to_owned()));
    }
    assert_eq!(publications, 1);
    assert_eq!(
        streams.next_tick(),
        Some(now + Duration::from_millis(1) + INTERVAL)
    );
    assert_eq!(streams.flush(|_, text| text.clone()), ["hello"]);
    assert!(streams.next_tick().is_none());
    assert!(streams.flush(|_, text| text.clone()).is_empty());
}

#[test]
fn terminal_results_discard_only_their_queued_partial() {
    let mut streams = Streams::default();
    let now = Instant::now();
    for id in ["a", "b"] {
        streams.insert(id, 0);
        streams.update(id, now, |_| true, |_, value| *value);
        streams.update(
            id,
            now,
            |value| {
                *value = 1;
                true
            },
            |_, value| *value,
        );
    }
    assert_eq!(streams.remove("a"), Some(1));
    assert_eq!(streams.next_tick(), Some(now + INTERVAL));
    assert_eq!(
        streams.flush(|id, value| (id.to_owned(), *value)),
        [("b".into(), 1)]
    );
    assert!(!streams.contains("a"));
    streams.update("b", now, |_| true, |_, value| *value);
    streams.clear();
    assert!(streams.next_tick().is_none());
    assert!(streams.flush(|_, value| *value).is_empty());
}

#[test]
fn unknown_or_unchanged_events_do_not_arm_a_timer() {
    struct NonDefault;
    let mut streams = Streams::<NonDefault>::default();
    let now = Instant::now();
    assert!(
        streams
            .update("unknown", now, |_| true, |_, _| ())
            .is_none()
    );
    streams.insert("known", NonDefault);
    assert!(streams.update("known", now, |_| false, |_, _| ()).is_none());
    assert!(streams.next_tick().is_none());
}

#[test]
fn capacity_and_identity_bounds_cannot_replace_existing_streams() {
    let mut streams = Streams::default();
    assert!(!streams.insert("", 0));
    assert!(!streams.insert(&"x".repeat(ID_BYTES + 1), 0));
    for n in 0..CAPACITY {
        assert!(streams.insert(&n.to_string(), n));
    }
    assert!(!streams.insert("overflow", 99));
    assert!(!streams.insert("0", 99));
    assert_eq!(streams.remove("0"), Some(0));
    assert!(streams.insert("overflow", 99));
}

#[test]
fn a_burst_does_not_rebuild_a_snapshot_for_every_byte() {
    let mut streams = Streams::default();
    streams.insert("shell", String::new());
    let now = Instant::now();
    let mut copies = 0;
    for _ in 0..100_000 {
        streams.update(
            "shell",
            now,
            |value| {
                value.push('x');
                true
            },
            |_, value| {
                copies += 1;
                value.clone()
            },
        );
    }
    let final_update = streams.flush(|_, value| {
        copies += 1;
        value.clone()
    });
    assert_eq!(copies, 2);
    assert_eq!(final_update, ["x".repeat(100_000)]);
    assert!(streams.next_tick().is_none());
}
