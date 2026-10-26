//! The start, update, and end decision table.

use super::*;
use crate::apns::decide::{DEFAULT_STEP, LINGER_S, current_step};
use herdr_chat_protocol::{
    Change, Cost, Entry, EntryId, EntryKind, Permission, PermissionId, PlanEntry, PlanPriority,
    PlanStatus, Status, Tool, ToolKind, ToolStatus, Transcript, Turn, TurnEnd, TurnSummary, Usage,
};

const NOW_MS: u64 = 1_800_000_000_000;
const STARTED_MS: u64 = 1_799_999_990_000;
/// `state.fingerprint`'s form: uppercase hex pairs joined by colons (here
/// with a lowercase pair, which the id normalizes too).
const FINGERPRINT: &str = "AB:01:AB:01:AB:01:AB:01:AB:01:AB:01:AB:01:AB:01:\
                           AB:01:AB:01:AB:01:AB:01:AB:01:AB:01:AB:01:ab:01";

fn idle() -> Transcript {
    let mut transcript = Transcript::new("Claude");
    transcript.status = Status::Idle;
    transcript
}

fn running() -> Transcript {
    let mut transcript = idle();
    transcript.status = Status::Working;
    transcript.turn = Some(Turn {
        started_ms: STARTED_MS,
    });
    transcript.entries.push(Entry {
        id: EntryId(1),
        kind: EntryKind::User {
            text: "split the tests".to_owned(),
            files: 0,
        },
    });
    transcript.usage = Some(Usage {
        cost: Some(Cost {
            currency: "USD".to_owned(),
            micros: 340_000,
        }),
        size: 200_000,
        used: 1_000,
    });
    transcript
}

fn tool(title: &str, status: ToolStatus) -> EntryKind {
    EntryKind::Tool(Tool {
        call_id: title.to_owned(),
        content: Vec::new(),
        kind: ToolKind::Execute,
        locations: Vec::new(),
        status,
        title: title.to_owned(),
    })
}

fn push(id: u64, kind: EntryKind) -> Change {
    Change::Push {
        entry: Entry {
            id: EntryId(id),
            kind,
        },
    }
}

fn permission(id: u64, title: &str) -> Permission {
    Permission {
        call_id: None,
        id: PermissionId(id),
        options: Vec::new(),
        title: title.to_owned(),
    }
}

fn plan(statuses: &[(&str, PlanStatus)]) -> Vec<PlanEntry> {
    statuses
        .iter()
        .map(|(content, status)| PlanEntry {
            content: (*content).to_owned(),
            priority: PlanPriority::Medium,
            status: *status,
        })
        .collect()
}

fn summary(end: TurnEnd) -> EntryKind {
    EntryKind::Turn(TurnSummary {
        duration_ms: 362_000,
        end,
        tokens: None,
    })
}

fn decide(before: &Transcript, change: &Change) -> Option<Live> {
    super::decide(&CHAT, "studio", FINGERPRINT, before, change, NOW_MS)
}

#[test]
fn a_turn_starting_starts_the_activity() {
    let change = Change::Turn {
        turn: Some(Turn {
            started_ms: STARTED_MS,
        }),
    };
    let mut before = idle();
    before.plan = plan(&[("old", PlanStatus::Completed)]);
    let live = decide(&before, &change).unwrap();
    let Event::Start { alert, attributes } = &live.event else {
        panic!("expected a start, got {:?}", live.event);
    };
    assert_eq!(alert.title, "Claude is working");
    assert_eq!(alert.body, "herdr-gpui \u{b7} remote plan");
    assert_eq!(attributes.computer, "studio");
    assert_eq!(
        attributes.computer_id,
        "AB01".repeat(16),
        "the daemon's colon form, as the app stores it"
    );
    assert_eq!(attributes.chat_title, "remote plan");
    assert_eq!(attributes.pane, "w1-p3");
    assert_eq!(live.state.phase, Phase::Working);
    assert_eq!(live.state.started_at, STARTED_MS / 1000);
    assert_eq!(live.state.step, DEFAULT_STEP);
    assert_eq!(live.state.plan_total, None, "the old turn's plan is gone");
    assert!(!live.urgent());

    // A turn already running does not start a second one.
    assert!(decide(&running(), &change).is_none());
}

#[test]
fn a_running_tool_call_updates_the_step_once() {
    let before = running();
    let live = decide(
        &before,
        &push(2, tool("Run just ci", ToolStatus::InProgress)),
    )
    .unwrap();
    assert_eq!(live.event, Event::Update { alert: None });
    assert_eq!(live.state.step, "Run just ci");
    assert_eq!(live.state.cost.as_deref(), Some("$0.34"));

    let mut shown = before.clone();
    shown.entries.push(Entry {
        id: EntryId(2),
        kind: tool("Run just ci", ToolStatus::InProgress),
    });
    let same = Change::Replace {
        entry: Entry {
            id: EntryId(2),
            kind: tool("Run just ci", ToolStatus::InProgress),
        },
    };
    assert!(decide(&shown, &same).is_none(), "same step, no update");
    let done = Change::Replace {
        entry: Entry {
            id: EntryId(2),
            kind: tool("Run just ci", ToolStatus::Completed),
        },
    };
    assert!(
        decide(&shown, &done).is_none(),
        "a finished call is no step"
    );
}

#[test]
fn tool_calls_outside_a_turn_do_nothing() {
    let change = push(2, tool("Read lib.rs", ToolStatus::InProgress));
    assert!(decide(&idle(), &change).is_none());
}

#[test]
fn the_plan_moving_updates_step_and_counts() {
    let before = running();
    let change = Change::Plan {
        plan: plan(&[
            ("Read the code", PlanStatus::Completed),
            ("Write the tests", PlanStatus::InProgress),
            ("Run them", PlanStatus::Pending),
        ]),
    };
    let live = decide(&before, &change).unwrap();
    assert_eq!(live.state.step, "Write the tests");
    assert_eq!(
        (live.state.plan_done, live.state.plan_total),
        (Some(1), Some(3))
    );

    let mut same = before;
    if let Change::Plan { plan } = &change {
        same.plan.clone_from(plan);
    }
    assert!(decide(&same, &change).is_none(), "an unchanged plan");
}

#[test]
fn a_question_turns_it_amber_with_an_alert() {
    let live = decide(
        &running(),
        &Change::PermissionAsked {
            permission: permission(1, "Run just ci"),
        },
    )
    .unwrap();
    assert_eq!(live.state.phase, Phase::Question);
    assert_eq!(live.state.question.as_deref(), Some("Run just ci"));
    let Event::Update { alert: Some(alert) } = &live.event else {
        panic!("expected an update with an alert, got {:?}", live.event);
    };
    assert_eq!(alert.title, "Claude needs an answer");
    assert!(live.urgent());
}

#[test]
fn an_auto_accepted_question_does_nothing() {
    let mut before = running();
    before.auto_accept = true;
    let change = Change::PermissionAsked {
        permission: permission(1, "Run just ci"),
    };
    assert!(decide(&before, &change).is_none());
}

#[test]
fn answering_the_last_question_goes_back_to_working() {
    let mut before = running();
    before.permissions = vec![permission(1, "a"), permission(2, "b")];
    let first = Change::PermissionResolved {
        id: PermissionId(1),
    };
    assert!(decide(&before, &first).is_none(), "one still waits");

    before.permissions = vec![permission(2, "b")];
    let last = Change::PermissionResolved {
        id: PermissionId(2),
    };
    let live = decide(&before, &last).unwrap();
    assert_eq!(live.state.phase, Phase::Working);
    assert_eq!(live.state.question, None);
}

#[test]
fn a_step_while_blocked_stays_a_question() {
    let mut before = running();
    before.permissions = vec![permission(1, "Run just ci")];
    let live = decide(&before, &push(2, tool("Edit lib.rs", ToolStatus::Pending))).unwrap();
    assert_eq!(live.state.phase, Phase::Question);
    assert_eq!(live.state.question.as_deref(), Some("Run just ci"));
}

#[test]
fn the_summary_ends_it_finished_and_lingering() {
    let live = decide(&running(), &push(3, summary(TurnEnd::Done))).unwrap();
    assert_eq!(
        live.event,
        Event::End {
            dismissal_s: NOW_MS / 1000 + LINGER_S
        }
    );
    assert_eq!(live.state.phase, Phase::Finished);
    assert_eq!(
        live.state.summary.as_deref(),
        Some("Worked 6m 2s \u{b7} $0.34")
    );
}

#[test]
fn a_failed_turn_ends_failed_and_a_cancelled_one_leaves_at_once() {
    let failed = decide(&running(), &push(3, summary(TurnEnd::Failed))).unwrap();
    assert_eq!(failed.state.phase, Phase::Failed);

    let cancelled = decide(&running(), &push(3, summary(TurnEnd::Cancelled))).unwrap();
    assert_eq!(
        cancelled.event,
        Event::End {
            dismissal_s: NOW_MS / 1000
        }
    );
}

#[test]
fn the_turn_ending_after_its_summary_needs_nothing_more() {
    let mut before = running();
    before.entries.push(Entry {
        id: EntryId(3),
        kind: summary(TurnEnd::Done),
    });
    assert!(decide(&before, &Change::Turn { turn: None }).is_none());
}

#[test]
fn the_turn_ending_without_a_summary_ends_it_failed() {
    let live = decide(&running(), &Change::Turn { turn: None }).unwrap();
    assert!(matches!(live.event, Event::End { .. }));
    assert_eq!(live.state.phase, Phase::Failed);
    assert_eq!(live.state.summary.as_deref(), Some("Claude stopped"));
}

#[test]
fn the_agent_failing_mid_turn_ends_it() {
    let change = Change::Status {
        status: Status::Failed {
            message: "the agent stopped answering".to_owned(),
        },
    };
    let live = decide(&running(), &change).unwrap();
    assert!(matches!(live.event, Event::End { .. }));
    assert_eq!(
        live.state.summary.as_deref(),
        Some("the agent stopped answering")
    );
    assert!(decide(&idle(), &change).is_none(), "no turn, no activity");
}

#[test]
fn current_step_prefers_the_running_call_then_the_plan_then_the_last_call() {
    let mut transcript = running();
    assert_eq!(current_step(&transcript), DEFAULT_STEP);
    transcript.entries.push(Entry {
        id: EntryId(2),
        kind: tool("Read lib.rs", ToolStatus::Completed),
    });
    assert_eq!(current_step(&transcript), "Read lib.rs");
    transcript.plan = plan(&[("Write the tests", PlanStatus::InProgress)]);
    assert_eq!(current_step(&transcript), "Write the tests");
    transcript.entries.push(Entry {
        id: EntryId(3),
        kind: tool("Run just ci", ToolStatus::InProgress),
    });
    assert_eq!(current_step(&transcript), "Run just ci");

    // Calls from before the last prompt belong to an older turn.
    transcript.plan.clear();
    transcript.entries.push(Entry {
        id: EntryId(4),
        kind: EntryKind::User {
            text: "next".to_owned(),
            files: 0,
        },
    });
    assert_eq!(current_step(&transcript), DEFAULT_STEP);
}
