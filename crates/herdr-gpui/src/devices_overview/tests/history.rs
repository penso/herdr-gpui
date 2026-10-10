use crate::devices_overview::history::{Counts, History, STEP, STEPS};
use std::time::{Duration, Instant, SystemTime};

fn counts(working: u16, blocked: u16) -> Counts {
    Counts { working, blocked }
}

#[test]
fn a_step_keeps_the_peak_seen_on_each_device() {
    let start = Instant::now();
    let mut history = History::default();
    let wall = SystemTime::UNIX_EPOCH;
    history.observe(
        start,
        wall,
        [("mac", counts(1, 0)), ("studio", counts(4, 1))],
    );
    history.observe(
        start + Duration::from_secs(10),
        wall + Duration::from_secs(10),
        [("mac", counts(3, 0))],
    );
    history.observe(
        start + Duration::from_secs(20),
        wall + Duration::from_secs(20),
        [("mac", counts(2, 2))],
    );
    assert_eq!(history.totals(), vec![counts(7, 3)]);
    assert_eq!(history.lane("mac"), vec![Some(counts(3, 2))]);
    assert_eq!(history.lane("gone"), vec![None]);
}

#[test]
fn steps_advance_on_the_step_clock() {
    let start = Instant::now();
    let mut history = History::default();
    let wall = SystemTime::UNIX_EPOCH;
    history.observe(start, wall, [("mac", counts(1, 0))]);
    history.observe(start + STEP, wall + STEP, [("mac", counts(2, 0))]);
    history.observe(
        start + STEP * 2 - Duration::from_secs(1),
        wall + STEP * 2 - Duration::from_secs(1),
        [("mac", counts(5, 0))],
    );
    assert_eq!(
        history.lane("mac"),
        vec![Some(counts(1, 0)), Some(counts(5, 0))]
    );
}

#[test]
fn a_device_that_was_not_connected_has_no_count() {
    let start = Instant::now();
    let mut history = History::default();
    let wall = SystemTime::UNIX_EPOCH;
    history.observe(
        start,
        wall,
        [("mac", counts(1, 0)), ("laptop", counts(1, 0))],
    );
    history.observe(start + STEP, wall + STEP, [("mac", counts(1, 0))]);
    assert_eq!(history.lane("laptop"), vec![Some(counts(1, 0)), None]);
}

#[test]
fn time_without_observations_is_kept_as_empty_steps() {
    let start = Instant::now();
    let mut history = History::default();
    let wall = SystemTime::UNIX_EPOCH;
    history.observe(start, wall, [("mac", counts(2, 0))]);
    // Asleep for three and a half steps.
    history.observe(
        start + STEP * 3 + STEP / 2,
        wall + STEP * 3 + STEP / 2,
        [("mac", counts(1, 0))],
    );
    assert_eq!(
        history.lane("mac"),
        vec![Some(counts(2, 0)), None, None, Some(counts(1, 0))]
    );
}

#[test]
fn history_is_bounded_even_after_a_long_sleep() {
    let start = Instant::now();
    let mut history = History::default();
    let wall = SystemTime::UNIX_EPOCH;
    for minute in 0..(STEPS as u32 * 3) {
        history.observe(
            start + STEP * minute,
            wall + STEP * minute,
            [("mac", counts(1, 0))],
        );
    }
    // Finished steps plus the one being counted.
    assert_eq!(history.totals().len(), STEPS + 1);
    history.observe(
        start + STEP * 100_000,
        wall + STEP * 100_000,
        [("mac", counts(9, 0))],
    );
    let lane = history.lane("mac");
    assert_eq!(lane.len(), STEPS + 1);
    assert_eq!(lane.last(), Some(&Some(counts(9, 0))));
    assert!(
        lane[..STEPS].iter().all(Option::is_none),
        "the sleep is empty"
    );
}
