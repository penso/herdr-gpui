//! When a run's page is read again: its agents keep spending after it ends.
use super::*;
use crate::automations::model::run_stale;

#[test]
fn a_finished_run_is_read_again_while_its_agents_may_spend() {
    let read = run(127, "nightly", RunState::Completed, NOW - HOUR);
    let ended = read.finished_at.unwrap();
    assert!(
        run_stale(None, Some(&read), ended + 60),
        "agents still counted"
    );
    assert!(
        !run_stale(Some(&read), Some(&read), ended + 2 * 86_400),
        "past a day, a finished page stands"
    );
    let mut no_agents = read.clone();
    no_agents.agents = 0;
    assert!(!run_stale(Some(&no_agents), Some(&no_agents), ended + 60));
    // The host reports more tokens than the page shows.
    let mut latest = no_agents.clone();
    latest.tokens = Some(2_000_000);
    assert!(run_stale(
        Some(&latest),
        Some(&no_agents),
        ended + 2 * 86_400
    ));
    let running = run(128, "nightly", RunState::Running, NOW);
    assert!(run_stale(Some(&running), None, NOW));
}
