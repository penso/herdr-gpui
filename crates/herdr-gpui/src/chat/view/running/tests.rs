#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;

fn activity(call: &str, kind: ActivityKind, state: ActivityState, started: u64) -> Activity {
    Activity {
        call_id: call.into(),
        detail: None,
        finished_ms: (state != ActivityState::Running).then_some(started + 41_000),
        kind,
        label: format!("{call} label"),
        started_ms: started,
        state,
    }
}

fn transcript(activities: Vec<Activity>) -> Transcript {
    Transcript {
        activities,
        ..Transcript::new("Claude")
    }
}

#[core::prelude::v1::test]
fn nothing_shows_once_nothing_runs() {
    assert_eq!(Running::of(&transcript(Vec::new()), 0), None);
    let ended = transcript(vec![activity(
        "a",
        ActivityKind::Shell,
        ActivityState::Finished,
        0,
    )]);
    assert_eq!(Running::of(&ended, 90_000), None);
}

#[core::prelude::v1::test]
fn the_summary_counts_what_runs_by_kind() {
    let running = Running::of(
        &transcript(vec![
            activity("a", ActivityKind::Subagent, ActivityState::Running, 0),
            activity("b", ActivityKind::Subagent, ActivityState::Running, 0),
            activity("c", ActivityKind::Shell, ActivityState::Running, 0),
            activity("d", ActivityKind::Shell, ActivityState::Killed, 0),
        ]),
        5_000,
    )
    .unwrap();
    assert_eq!(running.summary, "2 sub-agents \u{b7} 1 background shell");
    let one = Running::of(
        &transcript(vec![
            activity("a", ActivityKind::Shell, ActivityState::Running, 0),
            activity("b", ActivityKind::Other, ActivityState::Running, 0),
        ]),
        0,
    )
    .unwrap();
    assert_eq!(one.summary, "1 background shell \u{b7} 1 task");
}

#[core::prelude::v1::test]
fn running_rows_come_first_then_the_latest_to_end() {
    let running = Running::of(
        &transcript(vec![
            activity("old", ActivityKind::Shell, ActivityState::Finished, 0),
            activity(
                "live",
                ActivityKind::Subagent,
                ActivityState::Running,
                10_000,
            ),
            activity("new", ActivityKind::Shell, ActivityState::Failed, 50_000),
        ]),
        74_000,
    )
    .unwrap();
    let labels: Vec<&str> = running.rows.iter().map(|row| row.label.as_str()).collect();
    assert_eq!(labels, ["live label", "new label", "old label"]);
    assert_eq!(running.rows[0].elapsed, "1m 4s", "running counts to now");
    assert_eq!(running.rows[1].elapsed, "41s", "ended counts to its end");
    assert_eq!(running.rows[1].state, ActivityState::Failed);
}
