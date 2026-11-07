use crate::devices_overview::history::{Counts, History, STEP, STEPS};
use std::time::{Duration, Instant, SystemTime};

const ONE: Counts = Counts {
    working: 1,
    blocked: 0,
};

#[test]
fn suspension_advances_history_even_when_the_monotonic_clock_stops() {
    let (now, wall) = (Instant::now(), SystemTime::UNIX_EPOCH);
    let mut history = History::default();
    history.observe(now, wall, [("mac", ONE)]);
    history.observe(
        now + Duration::from_secs(1),
        wall + STEP * 3 + STEP / 2,
        [("mac", ONE)],
    );
    assert_eq!(history.lane("mac"), [Some(ONE), None, None, Some(ONE)]);
    // The half-minute remainder survives waking and regular ticks resume it.
    history.observe(
        now + Duration::from_secs(31),
        wall + STEP * 4,
        [("mac", ONE)],
    );
    assert_eq!(
        history.lane("mac"),
        [Some(ONE), None, None, Some(ONE), Some(ONE)]
    );
}

#[test]
fn overnight_suspend_or_large_forward_clock_adjustment_expires_old_counts() {
    let (now, wall) = (Instant::now(), SystemTime::UNIX_EPOCH);
    let mut history = History::default();
    history.observe(now, wall, [("mac", ONE)]);
    history.observe(
        now + Duration::from_secs(1),
        wall + Duration::from_secs(12 * 3600),
        [],
    );
    let lane = history.lane("mac");
    assert_eq!(lane.len(), STEPS + 1);
    assert!(lane.iter().all(Option::is_none));
}

#[test]
fn backward_wall_clock_adjustment_does_not_freeze_history() {
    let (now, wall) = (
        Instant::now(),
        SystemTime::UNIX_EPOCH + Duration::from_secs(3600),
    );
    let mut history = History::default();
    history.observe(now, wall, [("mac", ONE)]);
    history.observe(now + STEP, wall - STEP * 10, []);
    history.observe(now + STEP * 2, wall - STEP * 9, [("mac", ONE)]);
    assert_eq!(history.lane("mac"), [Some(ONE), None, Some(ONE)]);
}

#[test]
fn small_clock_slews_do_not_advance_the_step_early() {
    let (now, wall) = (Instant::now(), SystemTime::UNIX_EPOCH);
    let mut history = History::default();
    history.observe(now, wall, [("mac", ONE)]);
    history.observe(now + STEP - Duration::from_secs(1), wall + STEP, []);
    assert_eq!(history.lane("mac"), [Some(ONE)]);
    history.observe(now + STEP, wall + STEP + Duration::from_secs(1), []);
    assert_eq!(history.lane("mac"), [Some(ONE), None]);
}
