#![allow(clippy::unwrap_used)]

use super::*;
use sheprd_chat_protocol::{Entry, EntryId, Turn, TurnEnd, TurnSummary};

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
        started_ms: None,
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
            step("`cargo test -p sheprd-gpui`", ToolStatus::InProgress),
        ])),
        "cargo test -p sheprd-gpui"
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

fn tool(kind: ToolKind, title: &str, status: ToolStatus) -> Tool {
    Tool {
        call_id: title.into(),
        content: Vec::new(),
        kind,
        locations: Vec::new(),
        parent: None,
        started_ms: None,
        status,
        title: title.into(),
    }
}

fn prompt() -> EntryKind {
    EntryKind::User {
        text: "Fix it".into(),
        files: 0,
    }
}

#[core::prelude::v1::test]
fn a_step_on_a_file_names_it_rather_than_its_title() {
    let doing_on = |kind, location: &str| {
        let mut step = tool(
            kind,
            "A title long enough to lose the file",
            ToolStatus::InProgress,
        );
        step.locations = vec![location.into()];
        // Fluent isolates a value from the text around it.
        doing(&working(vec![EntryKind::Tool(step)])).replace(['\u{2068}', '\u{2069}'], "")
    };
    assert_eq!(
        doing_on(ToolKind::Edit, "/repo/apps/server/purpose.ts"),
        "Editing server/purpose.ts"
    );
    assert_eq!(doing_on(ToolKind::Read, "main.rs:12"), "Reading main.rs:12");
    assert_eq!(doing_on(ToolKind::Delete, "a/b/c"), "Deleting b/c");
    assert_eq!(doing_on(ToolKind::Move, "a/b/c"), "Moving b/c");
    assert_eq!(doing_on(ToolKind::Search, "a/b/c"), "Searching b/c");
    // A command's location is the directory it runs in.
    assert_eq!(
        doing_on(ToolKind::Execute, "/repo/worktree"),
        "A title long enough to lose the file"
    );
}

#[core::prelude::v1::test]
fn a_running_step_shows_even_when_a_later_one_finished_first() {
    assert_eq!(
        doing(&working(vec![
            prompt(),
            step("slow", ToolStatus::InProgress),
            step("fast", ToolStatus::Completed),
        ])),
        "slow"
    );
}

#[core::prelude::v1::test]
fn steps_beside_the_newest_are_counted_and_listed() {
    let shown = Working::of(
        &working(vec![
            prompt(),
            step("first", ToolStatus::InProgress),
            step("done", ToolStatus::Completed),
            step("second", ToolStatus::Pending),
            step("third", ToolStatus::InProgress),
        ]),
        0,
    )
    .unwrap();
    assert_eq!(shown.doing, "third");
    assert_eq!(shown.more, 2);
    let texts: Vec<_> = shown.steps.iter().map(|step| step.text.as_str()).collect();
    assert_eq!(texts, ["third", "second", "first"]);
}

#[core::prelude::v1::test]
fn a_sub_agent_gives_way_to_a_step_and_is_not_counted() {
    let agent = EntryKind::Tool(tool(ToolKind::Subagent, "Review", ToolStatus::InProgress));
    let waiting = Working::of(&working(vec![prompt(), agent.clone()]), 0).unwrap();
    assert_eq!((waiting.doing.as_str(), waiting.more), ("Review", 0));
    let stepping = Working::of(
        &working(vec![
            prompt(),
            step("cargo test", ToolStatus::InProgress),
            agent,
        ]),
        0,
    )
    .unwrap();
    assert_eq!((stepping.doing.as_str(), stepping.more), ("cargo test", 0));
    assert_eq!(stepping.steps.len(), 2);
}

#[core::prelude::v1::test]
fn an_earlier_turns_unfinished_step_does_not_count() {
    let stale = step("stale", ToolStatus::InProgress);
    assert_eq!(doing(&working(vec![stale.clone(), prompt()])), "Thinking");
    let ended = EntryKind::Turn(TurnSummary {
        duration_ms: 1,
        ended_ms: None,
        end: TurnEnd::Cancelled,
        tokens: None,
    });
    assert_eq!(doing(&working(vec![stale, ended])), "Thinking");
}

#[core::prelude::v1::test]
fn each_step_keeps_its_own_time_from_the_first_second() {
    let mut slow = tool(ToolKind::Execute, "slow", ToolStatus::InProgress);
    slow.started_ms = Some(1_000);
    let mut fresh = tool(ToolKind::Execute, "fresh", ToolStatus::InProgress);
    fresh.started_ms = Some(41_500);
    let unknown = tool(ToolKind::Execute, "unknown", ToolStatus::InProgress);
    let shown = Working::of(
        &working(vec![
            EntryKind::Tool(unknown),
            EntryKind::Tool(slow),
            EntryKind::Tool(fresh),
        ]),
        42_000,
    )
    .unwrap();
    assert_eq!(shown.step, None, "the newest has not run a second yet");
    assert_eq!(shown.steps[0].elapsed, None);
    assert!(
        shown.steps[1]
            .elapsed
            .as_ref()
            .is_some_and(|e| e.contains("41"))
    );
    assert_eq!(
        shown.steps[2].elapsed, None,
        "an older program sent no start"
    );
}

#[core::prelude::v1::test]
fn the_list_keeps_a_steps_whole_title_and_its_files() {
    let mut command = tool(
        ToolKind::Execute,
        "`sed one\nsed two`",
        ToolStatus::InProgress,
    );
    command.locations = vec!["/repo".into()];
    assert_eq!(step_text(&command), "sed one\nsed two");
    let mut edit = tool(ToolKind::Edit, "Edit a.rs", ToolStatus::InProgress);
    edit.locations = vec!["a.rs".into(), "src/b.rs".into()];
    assert_eq!(step_text(&edit), "Edit a.rs\nsrc/b.rs");
    let script = tool(
        ToolKind::Execute,
        &"é".repeat(MAX_STEP_TEXT + 10),
        ToolStatus::InProgress,
    );
    assert_eq!(step_text(&script).chars().count(), MAX_STEP_TEXT);
}

#[core::prelude::v1::test]
fn the_list_is_bounded_but_the_count_is_not() {
    let entries = (0..MAX_STEPS + 4)
        .map(|index| step(&format!("step {index}"), ToolStatus::InProgress))
        .collect();
    let shown = Working::of(&working(entries), 0).unwrap();
    assert_eq!(shown.steps.len(), MAX_STEPS);
    assert_eq!(shown.more, MAX_STEPS + 3);
}
