use super::*;
use crate::push::throttle::{CHAT_INTERVAL_MS, HOUR_MS, MAX_PER_HOUR};

fn notification(pane: &str, kind: Kind, body: &str) -> Notification {
    Notification {
        body: body.to_owned(),
        clear: false,
        kind,
        pane: pane.to_owned(),
        tag: format!("chat-{pane}"),
        title: "t".to_owned(),
        urgency: Urgency::Normal,
    }
}

fn bodies(notifications: &[Notification]) -> Vec<&str> {
    notifications.iter().map(|n| n.body.as_str()).collect()
}

#[test]
fn a_burst_for_one_chat_coalesces_to_the_latest() {
    let mut throttle = Throttle::default();
    let first = throttle.offer(notification("a", Kind::TurnEnded, "1"), 0);
    assert_eq!(first.map(|n| n.body), Some("1".to_owned()));
    assert_eq!(
        throttle.offer(notification("a", Kind::TurnEnded, "2"), 1_000),
        None
    );
    assert_eq!(
        throttle.offer(notification("a", Kind::Error, "3"), 2_000),
        None
    );

    assert!(throttle.due(CHAT_INTERVAL_MS - 1).is_empty());
    assert_eq!(bodies(&throttle.due(CHAT_INTERVAL_MS)), ["3"]);
    assert!(throttle.due(CHAT_INTERVAL_MS * 3).is_empty());
}

#[test]
fn chats_do_not_hold_each_other_back() {
    let mut throttle = Throttle::default();
    assert!(
        throttle
            .offer(notification("a", Kind::TurnEnded, "a"), 0)
            .is_some()
    );
    assert!(
        throttle
            .offer(notification("b", Kind::TurnEnded, "b"), 10)
            .is_some()
    );
}

#[test]
fn questions_and_clears_skip_the_wait_and_replace_what_was_held() {
    let mut throttle = Throttle::default();
    assert!(
        throttle
            .offer(notification("a", Kind::TurnEnded, "1"), 0)
            .is_some()
    );
    assert_eq!(
        throttle.offer(notification("a", Kind::Error, "2"), 100),
        None
    );
    let question = throttle.offer(notification("a", Kind::Question, "q"), 200);
    assert_eq!(question.map(|n| n.body), Some("q".to_owned()));
    // The held error was older than the question, which replaced it.
    assert!(throttle.due(CHAT_INTERVAL_MS * 2).is_empty());

    let clear = Notification {
        clear: true,
        ..notification("a", Kind::Question, "")
    };
    assert!(throttle.offer(clear, CHAT_INTERVAL_MS * 2 + 1).is_some());
}

#[test]
fn the_hourly_cap_holds_even_for_questions() {
    let mut throttle = Throttle::default();
    for index in 0..MAX_PER_HOUR {
        let pane = index.to_string();
        let sent = throttle.offer(notification(&pane, Kind::TurnEnded, "x"), index as u64);
        assert!(sent.is_some(), "{index}");
    }
    let late = MAX_PER_HOUR as u64;
    assert_eq!(
        throttle.offer(notification("q", Kind::Question, "q"), late),
        None
    );
    assert_eq!(
        throttle.offer(notification("t", Kind::TurnEnded, "t"), late),
        None
    );

    // An hour after the first push, one slot frees up.
    assert!(
        throttle
            .offer(notification("q", Kind::Question, "q"), HOUR_MS)
            .is_some()
    );
    assert_eq!(
        throttle.offer(notification("r", Kind::Question, "r"), HOUR_MS),
        None
    );
}

#[test]
fn held_pushes_are_dropped_when_the_cap_is_reached() {
    let mut throttle = Throttle::default();
    assert!(
        throttle
            .offer(notification("a", Kind::TurnEnded, "1"), 0)
            .is_some()
    );
    assert_eq!(
        throttle.offer(notification("a", Kind::TurnEnded, "2"), 1),
        None
    );
    for index in 1..MAX_PER_HOUR {
        let pane = format!("other{index}");
        assert!(
            throttle
                .offer(notification(&pane, Kind::Question, "q"), 2)
                .is_some()
        );
    }
    assert!(throttle.due(CHAT_INTERVAL_MS).is_empty());
}

#[test]
fn memory_stays_bounded() {
    let mut throttle = Throttle::default();
    for index in 0..10_000u64 {
        let pane = index.to_string();
        throttle.offer(notification(&pane, Kind::TurnEnded, "x"), index * 1_000);
    }
    assert!(throttle.remembered() <= MAX_PER_HOUR);
    throttle.due(10_000 * 1_000 + HOUR_MS);
    assert_eq!(throttle.remembered(), 0);
}
