use super::*;
use crate::overview::{Marks, marks::Hold};

#[test]
fn a_snooze_holds_a_row_until_its_time_then_it_wakes() {
    let mut marks = Marks::default();
    let waiting = key("codex");
    assert!(marks.snooze(&waiting, State::Blocked, at(1_000)));
    let mark = marks.get(&waiting).unwrap();
    assert_eq!(
        mark.hold(State::Blocked, at(999)),
        Some(Hold::Snoozed { until: at(1_000) })
    );
    assert_eq!(mark.hold(State::Blocked, at(1_000)), None);

    // The tick ends the hold for good and says it woke.
    assert!(!marks.wake([(&waiting, State::Blocked)], at(999)));
    assert!(marks.wake([(&waiting, State::Blocked)], at(1_000)));
    let mark = marks.get(&waiting).unwrap();
    assert!(mark.woke);
    assert_eq!(mark.hold(State::Blocked, at(1_000)), None);
    // Opening it is the look it asked for; nothing is left to remember.
    assert!(marks.seen(&waiting));
    assert!(marks.get(&waiting).is_none());
    assert!(!marks.seen(&waiting));
}

#[test]
fn a_hold_ends_when_its_row_needs_a_look_again() {
    let mut marks = Marks::default();
    let finished = key("claude");
    marks.settle(&finished, State::Done);
    let mark = marks.get(&finished).unwrap();
    // Going idle after finishing is no news.
    assert_eq!(mark.hold(State::Idle, at(0)), Some(Hold::Settled));
    // Working or asking again is.
    assert_eq!(mark.hold(State::Working, at(0)), None);
    assert_eq!(mark.hold(State::Blocked, at(0)), None);
    assert!(marks.wake([(&finished, State::Working)], at(0)));
    // A settled row that came back is not a woken snooze.
    assert!(marks.get(&finished).is_none());
}

#[test]
fn a_row_not_shown_keeps_its_mark() {
    let mut marks = Marks::default();
    let away = key("away");
    marks.snooze(&away, State::Blocked, at(10));
    // Its host may only be disconnected: no row, so nothing to compare.
    assert!(!marks.wake(std::iter::empty(), at(20)));
    assert!(marks.get(&away).is_some());
}

#[test]
fn pins_toggle_and_settling_unpins() {
    let mut marks = Marks::default();
    let pinned = key("claude");
    assert!(marks.toggle_pin(&pinned));
    assert!(marks.get(&pinned).unwrap().pinned);
    assert!(marks.settle(&pinned, State::Done));
    assert!(!marks.get(&pinned).unwrap().pinned);
    assert!(marks.release(&pinned));
    // Unpinned and released, the mark says nothing and is dropped.
    assert!(marks.get(&pinned).is_none());
    let revision = marks.revision();
    assert!(!marks.release(&pinned));
    assert_eq!(marks.revision(), revision);
}

#[test]
fn a_key_too_long_to_keep_gets_no_mark() {
    let mut marks = Marks::default();
    let long = Key::Agent {
        endpoint: "local".into(),
        checkout: "x".repeat(2000),
        agent: "claude".into(),
    };
    assert!(!marks.toggle_pin(&long));
    assert!(marks.get(&long).is_none());
}

#[test]
fn marks_are_saved_and_read_back() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("overview-marks.json");
    let mut marks = Marks::at(Some(path.clone()));
    marks.snooze(&key("codex"), State::Blocked, at(1_234));
    marks.toggle_pin(&Key::Run { id: "run-1".into() });
    marks.finish();
    let read = Marks::at(Some(path.clone()));
    assert_eq!(
        read.get(&key("codex")).unwrap().hold(State::Blocked, at(0)),
        Some(Hold::Snoozed { until: at(1_234) })
    );
    assert!(read.get(&Key::Run { id: "run-1".into() }).unwrap().pinned);

    // A damaged file starts empty rather than failing.
    std::fs::write(&path, b"{\"marks\": 3}").unwrap();
    assert!(Marks::at(Some(path)).get(&key("codex")).is_none());
}
