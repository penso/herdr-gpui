#![allow(clippy::unwrap_used)]

use super::*;
use core::prelude::v1::test;
use herdr_chat_protocol::{Entry, EntryId, Tokens, Tool, ToolKind, Turn, Usage};

fn working() -> Transcript {
    Transcript {
        agent: "Custom ACP".into(),
        status: Status::Working,
        turn: Some(Turn { started_ms: 1000 }),
        entries: vec![Entry {
            id: EntryId(1),
            kind: EntryKind::Thought {
                text: "Reported thought".into(),
            },
        }],
        ..Transcript::default()
    }
}

#[test]
fn connection_and_authoritative_status_override_historical_activity() {
    let mut transcript = working();
    assert_eq!(
        project(&transcript, true, false, 3000).phase,
        Phase::Thinking
    );
    for status in [
        Status::Idle,
        Status::Starting,
        Status::Failed {
            message: "Error".into(),
        },
        Status::Exited {
            message: String::new(),
        },
    ] {
        transcript.status = status;
        assert!(!project(&transcript, true, false, 3000).phase.moving());
    }
    transcript.status = Status::Working;
    assert_eq!(project(&transcript, true, true, 3000).phase, Phase::Waiting);
    assert_eq!(
        project(&transcript, false, false, 3000).phase,
        Phase::Reconnecting
    );
    assert!(project(&transcript, false, false, 3000).metrics.is_empty());
    transcript.turn = None;
    assert_eq!(
        project(&transcript, true, false, 3000).phase,
        Phase::Working
    );
}

#[test]
fn observed_turn_start_does_not_promote_old_thoughts_or_tools_to_current_activity() {
    let mut transcript = working();
    transcript.read_only = true;
    // The observer received a new start, but no new turn's content yet.
    transcript.turn = Some(Turn { started_ms: 2000 });
    assert_eq!(
        project(&transcript, true, false, 3000).phase,
        Phase::Working
    );
    transcript.read_only = false;
    assert_eq!(
        project(&transcript, true, false, 3000).phase,
        Phase::Thinking
    );
    transcript.entries.push(Entry {
        id: EntryId(2),
        kind: EntryKind::Tool(Tool {
            parent: None,
            call_id: "old-call".into(),
            kind: ToolKind::Execute,
            status: ToolStatus::InProgress,
            title: "Earlier tool".into(),
            content: Vec::new(),
            locations: Vec::new(),
        }),
    });
    transcript.read_only = true;
    let observed = project(&transcript, true, false, 3000);
    assert_eq!(observed.phase, Phase::Working);
    assert_eq!(
        observed.detail.as_deref(),
        Some("Last reported tool: Earlier tool")
    );
    transcript.read_only = false;
    let native = project(&transcript, true, false, 3000);
    assert_eq!(native.phase, Phase::Running);
    assert_eq!(native.detail.as_deref(), Some("Current tool: Earlier tool"));
    if let EntryKind::Tool(tool) = &mut transcript.entries.last_mut().unwrap().kind {
        tool.kind = ToolKind::Think;
    }
    assert_eq!(
        project(&transcript, true, false, 3000).phase,
        Phase::Thinking
    );
    transcript.turn = None;
    let unknown_turn = project(&transcript, true, false, 3000);
    assert_eq!(unknown_turn.phase, Phase::Working);
    assert_eq!(
        unknown_turn.detail.as_deref(),
        Some("Last reported tool: Earlier tool")
    );
}

#[test]
fn current_tool_details_are_bounded_and_do_not_cross_turn_boundaries() {
    let mut transcript = working();
    transcript.entries.push(Entry {
        id: EntryId(2),
        kind: EntryKind::Tool(Tool {
            parent: None,
            call_id: "id".into(),
            kind: ToolKind::Execute,
            status: ToolStatus::InProgress,
            title: format!("Run {}", "λ".repeat(1000)),
            content: Vec::new(),
            locations: Vec::new(),
        }),
    });
    let activity = project(&transcript, true, false, 3000);
    assert_eq!(activity.phase, Phase::Running);
    assert!(activity.detail.unwrap().chars().count() <= 160);
    transcript.entries.push(Entry {
        id: EntryId(3),
        kind: EntryKind::User {
            files: 0,
            text: "New request".into(),
        },
    });
    let activity = project(&transcript, true, false, 3000);
    assert_eq!(activity.phase, Phase::Working);
    assert!(activity.detail.is_none());
}

#[test]
fn counters_distinguish_elapsed_context_and_completed_usage() {
    let mut transcript = working();
    transcript.usage = Some(Usage {
        cost: None,
        used: 50000,
        size: 200000,
    });
    transcript.entries.insert(
        0,
        Entry {
            id: EntryId(0),
            kind: EntryKind::Turn(TurnSummary {
                duration_ms: 12000,
                end: TurnEnd::Done,
                tokens: Some(Tokens {
                    input: 100,
                    output: 350,
                    thought: 50,
                    total: 450,
                    cached: 0,
                }),
            }),
        },
    );
    let metrics = project(&transcript, true, false, 12000).metrics;
    assert_eq!(metrics[0], "Elapsed 11s");
    assert_eq!(metrics[1], "Context 50k / 200k");
    assert_eq!(
        metrics[2],
        "Last turn: 12s · 350 output tokens · 50 thinking"
    );
    assert!(
        !project(&transcript, true, false, 500)
            .metrics
            .iter()
            .any(|text| text.starts_with("Elapsed"))
    );
    assert!(
        !project(&transcript, true, true, 12000)
            .metrics
            .iter()
            .any(|text| text.starts_with("Elapsed"))
    );
    transcript.usage.as_mut().unwrap().size = 0;
    assert!(
        !project(&transcript, true, false, 12000)
            .metrics
            .iter()
            .any(|text| text.starts_with("Context"))
    );
}

#[test]
fn a_duration_record_without_an_outcome_does_not_claim_success_or_failure() {
    let mut transcript = Transcript::new("Claude");
    transcript.status = Status::Idle;
    transcript.entries.push(Entry {
        id: EntryId(1),
        kind: EntryKind::Turn(TurnSummary {
            duration_ms: 23000,
            end: TurnEnd::Other,
            tokens: None,
        }),
    });
    let activity = project(&transcript, true, false, 30000);
    assert_eq!(activity.phase, Phase::Idle);
    assert_eq!(activity.metrics, ["Last turn: 23s"]);
}

#[test]
fn semantic_colors_keep_text_contrast_and_pulse_is_bounded() {
    let config = crate::config::Config::default();
    for theme in [
        Theme::default(),
        config.theme_named("Catppuccin Latte", true).unwrap(),
    ] {
        let phases = [
            Phase::Running,
            Phase::Thinking,
            Phase::Waiting,
            Phase::Completed,
            Phase::Failed,
        ];
        let colors: std::collections::HashSet<_> =
            phases.iter().map(|phase| phase.color(&theme)).collect();
        assert_eq!(colors.len(), phases.len());
        for phase in phases {
            assert!(crate::contrast::ratio(phase.color(&theme), theme.background) >= 4.49);
        }
    }
    for index in 0..3 {
        for step in 0..=24 {
            assert!((0.15..=1.01).contains(&pulse(step as f32 / 24., index)));
        }
    }
    assert_ne!(pulse(0., 0), pulse(0.25, 0));
}
