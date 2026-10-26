use super::*;
use herdr_chat_protocol::{CommandId, Queued, TurnEnd, TurnSummary};

fn turn(entries: Vec<Entry>) -> Turn {
    turn_of(entries, |_| {})
}

fn turn_of(entries: Vec<Entry>, edit: impl FnOnce(&mut Transcript)) -> Turn {
    let mut transcript = Transcript::new("Claude");
    transcript.entries = entries;
    edit(&mut transcript);
    Turn::new(
        &row("p1"),
        &View {
            transcript,
            connected: true,
        },
    )
}

fn user(id: u64, text: &str) -> Entry {
    entry(
        id,
        EntryKind::User {
            text: text.into(),
            files: 0,
        },
    )
}

fn agent(id: u64, text: &str) -> Entry {
    entry(id, EntryKind::Agent { text: text.into() })
}

#[test]
fn a_phone_with_no_turn_gets_the_whole_turn() {
    let now = turn(vec![user(1, "hi"), agent(2, "hello")]);
    let TurnPatch::Whole(markup) = now.patch(None) else {
        panic!("expected the whole turn");
    };
    assert!(markup.starts_with(r#"<main id="chat-turn""#), "{markup}");
    assert!(markup.contains(r#"id="chat-entries""#));
    assert!(markup.contains(r#"id="entry-2""#));
    assert!(markup.contains(r#"id="chat-extras""#));
}

#[test]
fn streamed_text_sends_only_its_entry() {
    let before = turn(vec![user(1, "hi"), agent(2, "long step"), agent(3, "Hel")]);
    let now = turn(vec![
        user(1, "hi"),
        agent(2, "long step"),
        agent(3, "Hello"),
    ]);
    let TurnPatch::Parts { regions, appended } = now.patch(Some(&before)) else {
        panic!("expected parts");
    };
    assert_eq!(appended, None);
    let ids: Vec<_> = regions.iter().map(|(id, _)| id.as_str()).collect();
    assert_eq!(ids, ["entry-3"]);
    assert!(regions[0].1.contains("Hello"));
}

#[test]
fn new_entries_are_appended_after_the_ones_shown() {
    let before = turn(vec![user(1, "hi"), agent(2, "Hel")]);
    let now = turn(vec![user(1, "hi"), agent(2, "Hello"), agent(3, "More")]);
    let TurnPatch::Parts { regions, appended } = now.patch(Some(&before)) else {
        panic!("expected parts");
    };
    let ids: Vec<_> = regions.iter().map(|(id, _)| id.as_str()).collect();
    assert_eq!(ids, ["entry-2"]);
    let appended = appended.unwrap();
    assert!(appended.starts_with(r#"<div id="entry-3""#), "{appended}");
    assert!(!appended.contains("entry-2"));
}

#[test]
fn an_unchanged_turn_sends_nothing() {
    let before = turn(vec![user(1, "hi"), agent(2, "hello")]);
    let now = turn(vec![user(1, "hi"), agent(2, "hello")]);
    assert_eq!(
        now.patch(Some(&before)),
        TurnPatch::Parts {
            regions: Vec::new(),
            appended: None,
        }
    );
}

#[test]
fn a_changed_extra_sends_the_extras_region() {
    let before = turn(vec![user(1, "hi")]);
    let now = turn_of(vec![user(1, "hi")], |transcript| {
        transcript.queued.push(Queued {
            attachments: Vec::new(),
            id: CommandId::random(),
            text: "next".into(),
        });
    });
    let TurnPatch::Parts { regions, appended } = now.patch(Some(&before)) else {
        panic!("expected parts");
    };
    assert_eq!(appended, None);
    assert_eq!(regions.len(), 1);
    assert_eq!(regions[0].0, "chat-extras");
    assert!(regions[0].1.contains("next"));
}

#[test]
fn a_new_prompt_or_dropped_entries_replace_the_whole_turn() {
    let before = turn(vec![user(1, "hi"), agent(2, "hello")]);
    let next_turn = turn(vec![
        user(1, "hi"),
        agent(2, "hello"),
        entry(
            3,
            EntryKind::Turn(TurnSummary {
                duration_ms: 1,
                end: TurnEnd::Done,
                tokens: None,
            }),
        ),
        user(4, "again"),
    ]);
    assert!(matches!(
        next_turn.patch(Some(&before)),
        TurnPatch::Whole(_)
    ));
    let dropped = turn(vec![agent(2, "hello")]);
    assert!(matches!(dropped.patch(Some(&before)), TurnPatch::Whole(_)));
}
