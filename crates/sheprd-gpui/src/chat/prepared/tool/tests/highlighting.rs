use super::*;
use crate::chat::prepared::Prepared;
use sheprd_chat_protocol::{Entry, EntryId, ToolStatus, Transcript};

fn transcript() -> (Transcript, Vec<(EntryId, u64)>) {
    let mut transcript = Transcript::new("Agent");
    transcript.entries.push(Entry {
        id: EntryId(1),
        kind: EntryKind::Tool(sheprd_chat_protocol::Tool {
            call_id: "call".into(),
            content: vec![text(r#"{"command":"ls"}"#)],
            kind: ToolKind::Execute,
            locations: Vec::new(),
            parent: None,
            started_ms: None,
            status: ToolStatus::Completed,
            title: "`ls`".into(),
        }),
    });
    (transcript, vec![(EntryId(1), 1)])
}

#[test]
fn a_folded_cards_source_spends_no_grammar_budget_until_it_opens() {
    let (transcript, keys) = transcript();
    let mut prepared = Prepared::default();
    prepared.update(&transcript, &keys);
    let id = match &*prepared.body(EntryId(1)).unwrap() {
        Body::Tool(summary) => summary.previews[0].code.id,
        _ => panic!("a tool call prepares a tool body"),
    };
    assert!(!prepared.visuals.wants(id));

    prepared.set_expanded(&HashSet::from([EntryId(1)]));
    prepared.update(&transcript, &keys);
    assert!(prepared.visuals.wants(id));
    assert_eq!(prepared.drawing(id).collect::<Vec<_>>(), [EntryId(1)]);
}

#[test]
fn plain_output_never_asks_for_a_grammar() {
    let (mut transcript, keys) = transcript();
    if let EntryKind::Tool(tool) = &mut transcript.entries[0].kind {
        tool.content = vec![text("just output")];
    }
    let mut prepared = Prepared::default();
    prepared.set_expanded(&HashSet::from([EntryId(1)]));
    prepared.update(&transcript, &keys);
    let id = match &*prepared.body(EntryId(1)).unwrap() {
        Body::Tool(summary) => summary.previews[0].code.id,
        _ => panic!("a tool call prepares a tool body"),
    };
    assert!(!prepared.visuals.wants(id));
}
