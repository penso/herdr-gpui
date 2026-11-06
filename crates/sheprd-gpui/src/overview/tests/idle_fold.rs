use super::*;
use crate::overview::Sections;

const HOUR: u64 = 60 * 60;

fn idle(agent: &str, since: Option<u64>) -> Row {
    Row {
        since: since.map(at),
        ..row(agent, State::Idle)
    }
}

#[test]
fn agents_idle_for_an_hour_fold_and_newer_ones_stay_listed() {
    let rows = vec![
        idle("fresh", Some(2 * HOUR - 1)),
        idle("exact", Some(HOUR)),
        idle("old", Some(0)),
    ];
    let sections = Sections::of(rows, None, at(2 * HOUR));
    assert_eq!(sections.idle.len(), 1);
    assert_eq!(sections.idle[0].detail, "fresh");
    assert_eq!(sections.long_idle.len(), 2);
    assert!(!sections.is_empty());
}

#[test]
fn an_agent_this_client_never_saw_start_is_not_folded() {
    let sections = Sections::of(vec![idle("unknown", None)], None, at(10 * HOUR));
    assert_eq!(sections.idle.len(), 1);
    assert!(sections.long_idle.is_empty());
}

#[test]
fn a_clock_set_back_folds_nothing() {
    let sections = Sections::of(vec![idle("future", Some(5 * HOUR))], None, at(HOUR));
    assert_eq!(sections.idle.len(), 1);
}

#[test]
fn only_idle_agents_fold() {
    let old = Row {
        since: Some(at(0)),
        ..row("done", State::Done)
    };
    let sections = Sections::of(vec![old], None, at(10 * HOUR));
    assert_eq!(sections.ready.len(), 1);
    assert!(sections.long_idle.is_empty());
}

#[test]
fn folded_rows_take_actions_only_once_unfolded() {
    let rows = vec![idle("old", Some(0)), row("busy", State::Working)];
    let sections = Sections::of(rows, None, at(10 * HOUR));
    assert_eq!(sections.active(false).count(), 1);
    assert_eq!(sections.active(true).count(), 2);
}

#[test]
fn only_the_fold_is_empty_of_rows_when_all_agents_are_long_idle() {
    let sections = Sections::of(vec![idle("old", Some(0))], None, at(10 * HOUR));
    assert!(sections.idle.is_empty());
    assert!(!sections.is_empty());
    assert_eq!(sections.needing(), 0);
}
