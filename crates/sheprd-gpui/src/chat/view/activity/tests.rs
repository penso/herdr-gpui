#![allow(clippy::unwrap_used)]

use super::super::working::{Phase, Working};
use super::*;
use sheprd_chat_protocol::{Entry, EntryId, NoticeLevel, Tool, ToolKind, ToolStatus, TurnSummary};

fn with(status: Status, entries: Vec<EntryKind>) -> Transcript {
    let mut transcript = Transcript::new("Claude");
    transcript.status = status;
    transcript.entries = entries
        .into_iter()
        .enumerate()
        .map(|(index, kind)| Entry {
            id: EntryId(index as u64 + 1),
            kind,
        })
        .collect();
    transcript
}

fn ended(end: TurnEnd) -> EntryKind {
    EntryKind::Turn(TurnSummary {
        duration_ms: 1000,
        ended_ms: None,
        end,
        tokens: None,
    })
}

fn agent() -> EntryKind {
    EntryKind::Agent { text: "Hi".into() }
}

fn step(kind: ToolKind) -> EntryKind {
    EntryKind::Tool(Tool {
        call_id: "c".into(),
        content: Vec::new(),
        kind,
        locations: Vec::new(),
        parent: None,
        started_ms: None,
        status: ToolStatus::InProgress,
        title: "step".into(),
    })
}

fn phase(transcript: &Transcript) -> Phase {
    Working::of(transcript, 0).unwrap().phase
}

#[core::prelude::v1::test]
fn an_idle_agent_says_how_its_last_turn_ended() {
    for (end, state) in [
        (TurnEnd::Done, State::Completed),
        (TurnEnd::Failed, State::Failed),
        (TurnEnd::Cancelled, State::Stopped),
        (TurnEnd::Limit, State::Stopped),
        (TurnEnd::Refused, State::Stopped),
        (TurnEnd::Other, State::Idle),
    ] {
        let transcript = with(Status::Idle, vec![agent(), ended(end)]);
        assert_eq!(State::of(&transcript, false), state);
    }
}

#[core::prelude::v1::test]
fn only_a_turn_just_ended_counts() {
    assert_eq!(
        State::of(&with(Status::Idle, Vec::new()), false),
        State::Idle
    );
    let notice = EntryKind::Notice {
        answers: Vec::new(),
        details: None,
        level: NoticeLevel::Info,
        text: "Compacted".into(),
    };
    assert_eq!(
        State::of(
            &with(Status::Idle, vec![ended(TurnEnd::Done), notice]),
            false
        ),
        State::Completed,
        "a notice is not the agent going on"
    );
    assert_eq!(
        State::of(
            &with(Status::Idle, vec![ended(TurnEnd::Failed), agent()]),
            false
        ),
        State::Idle,
        "an old failure stops counting once the agent has gone on"
    );
}

#[core::prelude::v1::test]
fn a_past_turn_never_makes_a_running_agent_look_finished() {
    let transcript = with(Status::Working, vec![ended(TurnEnd::Done)]);
    assert_eq!(State::of(&transcript, false), State::Working);
}

#[core::prelude::v1::test]
fn waiting_outranks_working_but_not_a_dead_agent() {
    let working = with(Status::Working, Vec::new());
    assert_eq!(State::of(&working, true), State::Waiting);
    let failed = with(
        Status::Failed {
            message: "gone".into(),
        },
        Vec::new(),
    );
    assert_eq!(State::of(&failed, true), State::Failed);
    let exited = with(
        Status::Exited {
            message: String::new(),
        },
        Vec::new(),
    );
    assert_eq!(State::of(&exited, true), State::Exited);
}

#[core::prelude::v1::test]
fn every_state_reads_apart_without_its_color() {
    let transcript = with(Status::Idle, Vec::new());
    let shown = |state: State| (state.text(&transcript), state.icon());
    let settled = [
        State::Completed,
        State::Failed,
        State::Stopped,
        State::Waiting,
    ]
    .map(shown);
    for (index, state) in settled.iter().enumerate() {
        assert!(state.1.is_some(), "{state:?} has a mark");
        assert!(!settled[..index].contains(state), "{state:?} is its own");
    }
}

#[core::prelude::v1::test]
fn the_row_tells_thinking_from_writing_from_a_step() {
    assert_eq!(
        phase(&with(Status::Working, vec![step(ToolKind::Execute)])),
        Phase::Running
    );
    assert_eq!(
        phase(&with(Status::Working, vec![step(ToolKind::Think)])),
        Phase::Thinking
    );
    assert_eq!(phase(&with(Status::Working, vec![agent()])), Phase::Writing);
    assert_eq!(phase(&with(Status::Working, Vec::new())), Phase::Thinking);
}

#[core::prelude::v1::test]
fn an_observed_log_claims_no_more_than_that_the_agent_works() {
    let mut transcript = with(
        Status::Working,
        vec![EntryKind::Thought { text: "hm".into() }],
    );
    transcript.read_only = true;
    assert_eq!(phase(&transcript), Phase::Working);
    transcript.entries.push(Entry {
        id: EntryId(9),
        kind: agent(),
    });
    assert_eq!(phase(&transcript), Phase::Working);
    transcript.entries.push(Entry {
        id: EntryId(10),
        kind: step(ToolKind::Execute),
    });
    assert_eq!(phase(&transcript), Phase::Running, "a recorded step shows");
}

#[core::prelude::v1::test]
fn waiting_says_where_to_answer_and_keeps_the_turns_time() {
    let mut transcript = with(Status::Working, vec![step(ToolKind::Execute)]);
    transcript.turn = Some(sheprd_chat_protocol::Turn { started_ms: 0 });
    let working = Working::of(&transcript, 5000).unwrap();
    let answers = [
        ChatInput::Composer,
        ChatInput::ReadOnly,
        ChatInput::Terminal,
    ]
    .map(|input| working.clone().waiting(input));
    for (index, waiting) in answers.iter().enumerate() {
        assert_eq!(waiting.phase, Phase::Waiting);
        assert_eq!(waiting.elapsed, working.elapsed);
        assert_ne!(waiting.doing, working.doing);
        assert!(
            answers[..index]
                .iter()
                .all(|other| other.doing != waiting.doing)
        );
    }
}
