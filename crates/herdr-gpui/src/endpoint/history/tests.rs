#![allow(clippy::unwrap_used)]
use super::*;

/// A snapshot of `boot` with `panes` open and `focused` focused.
fn at(boot: &str, panes: &[&str], focused: Option<&str>) -> ClientShellSnapshot {
    let mut snapshot: ClientShellSnapshot = serde_json::from_str(include_str!(
        "../../../../herdr-protocol/tests/fixtures/endpoint-snapshot-v1.json"
    ))
    .unwrap();
    let pane = snapshot.panes[0].clone();
    snapshot.panes = panes
        .iter()
        .map(|id| {
            let mut pane = pane.clone();
            pane.pane_id = (*id).to_owned();
            pane
        })
        .collect();
    snapshot.boot_id = boot.into();
    snapshot.focused_pane_id = focused.map(str::to_owned);
    snapshot
}

const PANES: &[&str] = &["a", "b", "c", "d"];

/// A snapshot focusing `focused` while a navigation may still be in flight.
fn visit(history: &mut History, focused: &str) -> ClientShellSnapshot {
    let snapshot = at("boot", PANES, Some(focused));
    history.observe(Some(&snapshot), true);
    snapshot
}

/// Starts `step` as the window does once its request is queued.
fn start(history: &mut History, step: Step, snapshot: &ClientShellSnapshot) -> Option<String> {
    let (index, pane) = history
        .peek(step, snapshot)
        .map(|(index, pane)| (index, pane.to_owned()))?;
    history.begin(index);
    Some(pane)
}

/// Goes `step`, then reports the focus arriving and the navigation settling.
fn go(history: &mut History, step: Step, snapshot: &ClientShellSnapshot) -> String {
    let pane = start(history, step, snapshot).unwrap();
    history.observe(Some(&at("boot", PANES, Some(&pane))), false);
    pane
}

#[test]
fn back_and_forward_walk_the_focus_trail() {
    let mut history = History::default();
    let mut now = visit(&mut history, "a");
    assert!(!history.can(Step::Back, &now));
    for pane in ["b", "c"] {
        now = visit(&mut history, pane);
    }
    assert!(!history.can(Step::Forward, &now));
    assert_eq!(go(&mut history, Step::Back, &now), "b");
    now = at("boot", PANES, Some("b"));
    assert_eq!(go(&mut history, Step::Back, &now), "a");
    now = at("boot", PANES, Some("a"));
    assert!(!history.can(Step::Back, &now));
    assert_eq!(go(&mut history, Step::Forward, &now), "b");
    now = at("boot", PANES, Some("b"));
    assert_eq!(go(&mut history, Step::Forward, &now), "c");
}

#[test]
fn going_somewhere_new_drops_the_entries_ahead() {
    let mut history = History::default();
    for pane in ["a", "b", "c"] {
        visit(&mut history, pane);
    }
    let now = at("boot", PANES, Some("c"));
    go(&mut history, Step::Back, &now);
    let now = visit(&mut history, "d");
    assert!(!history.can(Step::Forward, &now));
    assert_eq!(go(&mut history, Step::Back, &now), "b");
}

#[test]
fn a_second_step_before_the_first_lands_continues_from_it() {
    let mut history = History::default();
    for pane in ["a", "b", "c"] {
        visit(&mut history, pane);
    }
    let now = at("boot", PANES, Some("c"));
    assert_eq!(start(&mut history, Step::Back, &now).as_deref(), Some("b"));
    assert_eq!(start(&mut history, Step::Back, &now).as_deref(), Some("a"));
    // A snapshot that changes something else keeps the travel pending.
    visit(&mut history, "c");
    visit(&mut history, "a");
    let now = at("boot", PANES, Some("a"));
    assert_eq!(history.peek(Step::Forward, &now), Some((1, "b")));
}

#[test]
fn peeking_changes_nothing_until_the_request_is_queued() {
    let mut history = History::default();
    for pane in ["a", "b", "c"] {
        visit(&mut history, pane);
    }
    let now = at("boot", PANES, Some("c"));
    // The first press is queued; a second one the window cannot send only
    // peeks, so the first still lands as a step back.
    assert_eq!(start(&mut history, Step::Back, &now).as_deref(), Some("b"));
    assert_eq!(history.peek(Step::Back, &now), Some((0, "a")));
    history.observe(Some(&at("boot", PANES, Some("b"))), false);
    assert_eq!(
        history.peek(Step::Forward, &at("boot", PANES, Some("b"))),
        Some((2, "c"))
    );
}

#[test]
fn a_travel_ends_with_its_navigation() {
    for settles in [
        // The daemon answered without focusing the pane, say it had closed.
        Some(at("boot", PANES, Some("c"))),
        // The connection dropped and the next one starts empty.
        None,
    ] {
        let mut history = History::default();
        for pane in ["a", "b", "c"] {
            visit(&mut history, pane);
        }
        let now = at("boot", PANES, Some("c"));
        start(&mut history, Step::Back, &now);
        history.observe(settles.as_ref(), false);
        // The same boot returns on the old focus: Back starts from there
        // again, rather than from the pane that was never reached.
        history.observe(Some(&now), false);
        assert_eq!(history.peek(Step::Back, &now), Some((1, "b")));
        assert!(!history.can(Step::Forward, &now));
    }
}

#[test]
fn other_focus_ends_a_travel() {
    let mut history = History::default();
    for pane in ["a", "b", "c"] {
        visit(&mut history, pane);
    }
    let now = at("boot", PANES, Some("c"));
    start(&mut history, Step::Back, &now);
    // Focus moving elsewhere first is a new place, not the travel landing.
    let now = visit(&mut history, "d");
    assert!(!history.can(Step::Forward, &now));
    assert_eq!(go(&mut history, Step::Back, &now), "c");
}

#[test]
fn closed_and_focused_panes_are_stepped_over() {
    let mut history = History::default();
    for pane in ["a", "b", "a", "c"] {
        visit(&mut history, pane);
    }
    let closed = at("boot", &["a", "c"], Some("c"));
    assert_eq!(history.peek(Step::Back, &closed), Some((2, "a")));
    // Only the focused pane is left behind it.
    let alone = at("boot", &["c"], Some("c"));
    assert!(!history.can(Step::Back, &alone));
}

#[test]
fn the_trail_belongs_to_one_boot_and_survives_a_dropped_connection() {
    let mut history = History::default();
    visit(&mut history, "a");
    let now = visit(&mut history, "b");
    history.observe(None, false);
    history.observe(Some(&at("boot", PANES, None)), false);
    assert!(history.can(Step::Back, &now));
    // A snapshot from another boot cannot use this boot's trail.
    assert!(!history.can(Step::Back, &at("reboot", PANES, Some("b"))));
    let rebooted = at("reboot", PANES, Some("b"));
    history.observe(Some(&rebooted), false);
    assert!(!history.can(Step::Back, &rebooted));
}

#[test]
fn the_trail_is_bounded() {
    let mut history = History::default();
    let panes: Vec<String> = (0..LIMIT + 10).map(|i| format!("p{i}")).collect();
    for pane in &panes {
        history.observe(Some(&at("boot", &[pane.as_str()], Some(pane))), false);
    }
    assert_eq!(history.panes.len(), LIMIT);
    assert_eq!(history.panes.front().map(String::as_str), Some("p10"));
    assert_eq!(history.cursor, LIMIT - 1);
}
