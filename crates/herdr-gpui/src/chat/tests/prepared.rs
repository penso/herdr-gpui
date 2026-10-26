use super::*;
use crate::chat::prepared::{Body, DiffSummary, Part, Prepared, diff_counts, summarize};
use herdr_chat_protocol::{Tool, ToolContent, ToolKind, ToolStatus};

mod cut;
mod fences;

#[test]
fn literal_indented_widget_examples_cannot_become_controls() {
    let json = r#"{"components":[{"type":"text","text":"Example"}]}"#;
    for text in [
        format!("    ```herdr-ui\n    {json}\n    ```"),
        format!("\t```herdr-ui\n\t{json}\n\t```"),
        format!("```example`herdr-ui\n{json}\n```"),
    ] {
        let (prepared, _) = visual::message(&text);
        let body = prepared.body(EntryId(1)).unwrap();
        let Body::Message(parts) = &*body else {
            panic!("message");
        };
        assert!(!parts.iter().any(|part| matches!(part, Part::Widget(_))));
    }
}

#[test]
fn diffs_count_lines_added_and_removed() {
    assert_eq!(diff_counts("a\nb\nc\n", "a\nB\nc\nd\n"), (2, 1));
    assert_eq!(diff_counts("", "one\ntwo"), (2, 0));
    assert_eq!(diff_counts("same", "same"), (0, 0));
}

#[test]
fn a_tool_call_is_summarized() {
    let text = (0..10)
        .map(|n| format!("line {n}"))
        .collect::<Vec<_>>()
        .join("\n");
    let summary = summarize(
        &[
            ToolContent::Text { text },
            ToolContent::Diff {
                new: "x\ny\n".into(),
                old: None,
                path: "/src/new.rs".into(),
            },
            ToolContent::Terminal { id: "t1".into() },
        ],
        &mut Default::default(),
    );
    assert_eq!(summary.lines.len(), 6);
    assert_eq!(summary.more, 4);
    assert_eq!(
        summary.diffs,
        vec![DiffSummary {
            path: "/src/new.rs".into(),
            added: 2,
            removed: 0
        }]
    );
    assert_eq!(summary.terminals, 1);
}

#[test]
fn bodies_are_prepared_once_per_revision() {
    let mut transcript = Transcript::new("Claude");
    transcript.entries = vec![
        agent(1, "**bold** text"),
        Entry {
            id: EntryId(2),
            kind: EntryKind::Tool(Tool {
                call_id: "c".into(),
                content: Vec::new(),
                kind: ToolKind::Read,
                locations: Vec::new(),
                status: ToolStatus::Completed,
                title: "Read".into(),
            }),
        },
        Entry {
            id: EntryId(3),
            kind: EntryKind::User {
                text: "hi".into(),
                files: 0,
            },
        },
    ];
    let keys = vec![(EntryId(1), 1), (EntryId(2), 1), (EntryId(3), 1)];
    let mut prepared = Prepared::default();
    prepared.update(&transcript, &keys);
    let first = prepared.body(EntryId(1)).unwrap();
    assert!(matches!(&*first, Body::Message(parts)
        if matches!(&parts[..], [Part::Rich(blocks)]
            if matches!(&blocks[0], rich::Block::Paragraph { content, .. }
                if matches!(&content[0], rich::Inline::Text(text, style) if text == "bold" && style.bold)))));
    assert!(matches!(
        &*prepared.body(EntryId(2)).unwrap(),
        Body::Tool(_)
    ));
    assert!(matches!(
        &*prepared.body(EntryId(3)).unwrap(),
        Body::Message(_)
    ));
    prepared.update(&transcript, &keys);
    assert!(std::rc::Rc::ptr_eq(
        &first,
        &prepared.body(EntryId(1)).unwrap()
    ));
    transcript.entries.remove(0);
    prepared.update(&transcript, &keys[1..]);
    assert!(prepared.body(EntryId(1)).is_none());
}

#[test]
fn an_answer_too_long_to_show_ends_with_a_note() {
    let text = (0..3_000)
        .map(|n| format!("line {n}"))
        .collect::<Vec<_>>()
        .join("\n");
    let mut transcript = Transcript::new("Claude");
    transcript.entries = vec![agent(1, &text)];
    let mut prepared = Prepared::default();
    prepared.update(&transcript, &[(EntryId(1), 1)]);
    let Body::Message(parts) = &*prepared.body(EntryId(1)).unwrap() else {
        panic!("not a message");
    };
    let Part::Text(lines) = parts.last().unwrap() else {
        panic!("not text");
    };
    let Part::Rich(blocks) = &parts[0] else {
        panic!("not structured prose");
    };
    assert!(
        matches!(&blocks[0], rich::Block::Paragraph { content, .. }
        if content.iter().any(|part| matches!(part, rich::Inline::Text(text, _)
            if text.contains("line 1999")))),
        "chat shows more than release notes do"
    );
    assert!(lines.last().unwrap().text.contains("Copy takes all of it"));
    // A short answer has no note.
    transcript.entries = vec![agent(2, "short")];
    prepared.update(&transcript, &[(EntryId(2), 1)]);
    let Body::Message(parts) = &*prepared.body(EntryId(2)).unwrap() else {
        panic!("not a message");
    };
    assert!(matches!(&parts[..], [Part::Rich(blocks)] if blocks.len() == 1));
}
