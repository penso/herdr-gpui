use super::*;
use herdr_chat_protocol::{Entry, EntryId, Tool, ToolKind, Turn};

fn working(entries: Vec<EntryKind>) -> Transcript {
    let mut transcript = Transcript::new("Claude");
    transcript.status = Status::Working;
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

fn step(title: &str, status: ToolStatus) -> EntryKind {
    EntryKind::Tool(Tool {
        call_id: "c".into(),
        content: Vec::new(),
        kind: ToolKind::Execute,
        locations: Vec::new(),
        parent: None,
        status,
        title: title.into(),
    })
}

fn doing(transcript: &Transcript) -> String {
    Working::of(transcript, 0)
        .map(|working| working.doing)
        .unwrap_or_default()
}

#[core::prelude::v1::test]
fn nothing_shows_unless_a_turn_runs() {
    let mut transcript = working(Vec::new());
    transcript.status = Status::Idle;
    assert_eq!(Working::of(&transcript, 0), None);
}

#[core::prelude::v1::test]
fn it_says_what_the_agent_is_doing() {
    let prompt = EntryKind::User {
        text: "Fix it".into(),
        files: 0,
    };
    assert_eq!(doing(&working(vec![prompt.clone()])), "Thinking");
    assert_eq!(
        doing(&working(vec![
            prompt.clone(),
            EntryKind::Thought { text: "hm".into() }
        ])),
        "Thinking"
    );
    assert_eq!(
        doing(&working(vec![
            prompt.clone(),
            EntryKind::Agent {
                text: "Look".into()
            }
        ])),
        "Writing"
    );
    assert_eq!(
        doing(&working(vec![
            prompt.clone(),
            step("`cargo test -p herdr-gpui`", ToolStatus::InProgress),
        ])),
        "cargo test -p herdr-gpui"
    );
    // A finished step means the agent has moved on to thinking again.
    assert_eq!(
        doing(&working(vec![prompt, step("ls", ToolStatus::Completed)])),
        "Thinking"
    );
}

#[core::prelude::v1::test]
fn a_step_title_keeps_inner_ticks_and_its_first_line() {
    assert_eq!(step_title("`a` then `b`"), "`a` then `b`");
    assert_eq!(step_title("run\nmore"), "run");
    assert_eq!(step_title("  "), "Working");
}

#[core::prelude::v1::test]
fn the_elapsed_time_shows_from_the_first_second() {
    let mut transcript = working(Vec::new());
    transcript.turn = Some(Turn { started_ms: 10_000 });
    assert_eq!(
        Working::of(&transcript, 10_500).and_then(|w| w.elapsed),
        None
    );
    assert!(
        Working::of(&transcript, 22_000)
            .and_then(|w| w.elapsed)
            .is_some_and(|elapsed| elapsed.contains("12"))
    );
}
