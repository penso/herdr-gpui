#![allow(clippy::unwrap_used)]

use super::*;
use crate::chat::prepared::{Body, Part, Prepared};
use crate::chat::rich::{MAX_CODE_LINE_CHARS, MAX_CODE_LINES};
use herdr_chat_protocol::{Entry, EntryKind, NoticeLevel, QuestionAnswer, Transcript};
use std::{collections::HashMap, rc::Rc, sync::Arc};

fn agent(id: u64, text: &str) -> Entry {
    Entry {
        id: EntryId(id),
        kind: EntryKind::Agent {
            text: text.to_owned(),
        },
    }
}

fn prepare(entries: Vec<Entry>) -> (Transcript, Prepared, Vec<(EntryId, u64)>) {
    let mut transcript = Transcript::new("Test agent");
    transcript.entries = entries;
    let keys = transcript
        .entries
        .iter()
        .map(|entry| (entry.id, 1))
        .collect::<Vec<_>>();
    let mut prepared = Prepared::default();
    prepared.update(&transcript, &keys);
    (transcript, prepared, keys)
}

fn caret(
    document: &Document,
    entry: u64,
    block: usize,
    row: usize,
    column: usize,
    offset: usize,
) -> Caret {
    Caret {
        key: document
            .key(EntryId(entry), Region::Body(0), Cell { block, row, column })
            .unwrap(),
        offset,
    }
}

#[test]
fn unicode_reverse_selection_uses_transcript_order_and_readable_markdown() {
    let (_, prepared, _) = prepare(vec![agent(90, "**café** 東京"), agent(2, "next 🦀 end")]);
    let document = prepared.text();
    let start = caret(document, 90, 0, 0, 0, 3);
    let end = caret(document, 2, 0, 0, 0, 9);
    let mut selection = Selection::default();
    selection.press(document, end, 1, false);
    selection.drag(document, start);
    assert_eq!(
        selection.text(document).as_deref(),
        Some("é 東京\n\nnext 🦀")
    );
    assert_eq!(selection.highlight(document, start.key), Some(3..12));
    assert_eq!(selection.highlight(document, end.key), Some(0..9));
    selection.release();
    assert!(!selection.drag(document, Caret { offset: 0, ..start }));
}

#[test]
fn invalid_utf8_offsets_are_floored_and_word_shift_line_selection_work() {
    let (_, prepared, _) = prepare(vec![agent(1, "café 東京_test !")]);
    let document = prepared.text();
    let mut selection = Selection::default();
    let key = caret(document, 1, 0, 0, 0, 4);
    selection.press(document, key, 2, false);
    assert_eq!(selection.text(document).as_deref(), Some("café"));
    selection.press(document, Caret { offset: 17, ..key }, 1, true);
    assert_eq!(selection.text(document).as_deref(), Some("café 東京_test"));
    selection.press(document, Caret { offset: 6, ..key }, 3, false);
    assert_eq!(
        selection.text(document).as_deref(),
        Some("café 東京_test !")
    );
    assert!(selection.clear());
    assert!(selection.is_empty());
    assert!(!selection.clear());
}

#[test]
fn blocks_code_lines_and_table_cells_share_one_selection() {
    let (_, prepared, _) = prepare(vec![agent(
        1,
        "Intro\n\n```rust\nlet x = 1;\n\tx\n```\n\n| A | B |\n| - | - |\n| 3 | 4 |",
    )]);
    let document = prepared.text();
    let mut selection = Selection::default();
    selection.press(document, caret(document, 1, 0, 0, 0, 2), 1, false);
    selection.drag(document, caret(document, 1, 2, 1, 1, 1));
    assert_eq!(
        selection.text(document).as_deref(),
        Some("tro\n\nlet x = 1;\n\tx\n\nA\tB\n3\t4")
    );
    selection.select_all(document);
    assert_eq!(
        selection.text(document).as_deref(),
        Some("Intro\n\nlet x = 1;\n\tx\n\nA\tB\n3\t4")
    );
}

#[test]
fn code_tab_map_preserves_original_source_and_unicode() {
    let (_, prepared, _) = prepare(vec![agent(1, "```text\nα\tb\n中\tc\n```")]);
    let document = prepared.text();
    let first = caret(document, 1, 0, 0, 0, 2);
    let segment = document.segment(first.key).unwrap();
    assert_eq!(segment.text(), "α   b");
    assert_eq!(segment.code().unwrap().shown(3), Some(5));
    assert_eq!(segment.source_offset(4), 2);
    let mut selection = Selection::default();
    selection.press(document, first, 1, false);
    selection.drag(document, Caret { offset: 5, ..first });
    assert_eq!(selection.text(document).as_deref(), Some("\t"));
    selection.drag(document, caret(document, 1, 0, 1, 0, 6));
    assert_eq!(selection.text(document).as_deref(), Some("\tb\n中\tc"));
}

#[test]
fn inline_math_is_one_atomic_tex_range() {
    let (_, prepared, _) = prepare(vec![agent(1, "Before $\\alpha+1$ after.\n\n$$\nx^2\n$$")]);
    let document = prepared.text();
    let start = caret(document, 1, 0, 0, 0, 0);
    let segment = document.segment(start.key).unwrap();
    assert_eq!(segment.text(), "Before \\alpha+1 after.");
    assert_eq!(segment.atomic(), std::slice::from_ref(&(7..15)));
    let mut selection = Selection::default();
    selection.press(
        document,
        Caret {
            offset: 13,
            ..start
        },
        2,
        false,
    );
    assert_eq!(selection.text(document).as_deref(), Some("\\alpha+1"));
    selection.press(document, start, 1, false);
    selection.drag(
        document,
        Caret {
            offset: 14,
            ..start
        },
    );
    assert_eq!(
        selection.text(document).as_deref(),
        Some("Before \\alpha+1")
    );
    selection.select_all(document);
    assert!(
        selection
            .text(document)
            .unwrap()
            .trim_end()
            .ends_with("x^2")
    );
}

#[test]
fn unchanged_projection_is_shared_and_append_preserves_selection() {
    let (mut transcript, mut prepared, mut keys) = prepare(vec![agent(7, "stable")]);
    let projected = prepared.text().entries[0].clone();
    let mut selection = Selection::default();
    selection.select_all(prepared.text());
    transcript.entries.push(agent(1, "later"));
    keys.push((EntryId(1), 1));
    prepared.update(&transcript, &keys);
    assert!(Rc::ptr_eq(&projected, &prepared.text().entries[0]));
    assert!(!selection.validate(prepared.text()));
    assert_eq!(selection.text(prepared.text()).as_deref(), Some("stable"));
    transcript.entries[1] = agent(1, "later changed");
    keys[1].1 += 1;
    prepared.update(&transcript, &keys);
    assert!(!selection.validate(prepared.text()));
}

#[test]
fn changing_an_interior_selected_entry_or_reordering_invalidates() {
    let (mut transcript, mut prepared, mut keys) =
        prepare(vec![agent(9, "one"), agent(3, "two"), agent(8, "three")]);
    let mut selection = Selection::default();
    selection.select_all(prepared.text());
    transcript.entries[1] = agent(3, "changed");
    keys[1].1 += 1;
    prepared.update(&transcript, &keys);
    assert!(selection.text(prepared.text()).is_none());
    assert!(selection.validate(prepared.text()));
    selection.select_all(prepared.text());
    transcript.entries.swap(0, 1);
    keys.swap(0, 1);
    prepared.update(&transcript, &keys);
    assert!(selection.validate(prepared.text()));
}

#[test]
fn snapshot_reset_fences_old_keys_even_when_ids_and_revisions_are_reused() {
    let (transcript, mut prepared, keys) = prepare(vec![agent(1, "same")]);
    let old = caret(prepared.text(), 1, 0, 0, 0, 1);
    let mut selection = Selection::default();
    selection.select_all(prepared.text());
    prepared.clear();
    prepared.update(&transcript, &keys);
    assert!(selection.validate(prepared.text()));
    assert!(!selection.press(prepared.text(), old, 1, false));
    assert_ne!(old.key, caret(prepared.text(), 1, 0, 0, 0, 1).key);
}

#[test]
fn remote_file_destinations_match_visible_expanded_labels() {
    let (transcript, mut prepared, keys) =
        prepare(vec![agent(1, "Read [**source** file](src/main.rs:5).")]);
    let old = caret(prepared.text(), 1, 0, 0, 0, 0);
    prepared.set_remote_links(true);
    prepared.update(&transcript, &keys);
    assert!(prepared.text().segment(old.key).is_none());
    let new = caret(prepared.text(), 1, 0, 0, 0, 0);
    assert_eq!(
        prepared.text().segment(new.key).unwrap().text(),
        "Read source file (src/main.rs:5)."
    );
}

#[test]
fn notice_answers_and_inert_source_have_distinct_segments() {
    let (transcript, mut prepared, keys) = prepare(vec![Entry {
        id: EntryId(4),
        kind: EntryKind::Notice {
            answers: vec![QuestionAnswer {
                question: "Use café?".into(),
                answer: "Yes.".into(),
            }],
            details: Some("<record>source</record>".into()),
            level: NoticeLevel::Info,
            text: "Questions answered".into(),
        },
    }]);
    let mut selection = Selection::default();
    selection.select_all(prepared.text());
    assert!(
        !selection
            .text(prepared.text())
            .unwrap()
            .contains("<record>")
    );
    prepared.set_expanded(&std::collections::HashSet::from([EntryId(4)]));
    prepared.update(&transcript, &keys);
    assert!(selection.validate(prepared.text()));
    let document = prepared.text();
    assert_eq!(
        document
            .segment(
                document
                    .key(EntryId(4), Region::Label, Cell::default())
                    .unwrap()
            )
            .unwrap()
            .text(),
        "Questions answered"
    );
    assert_eq!(
        document
            .segment(
                document
                    .key(EntryId(4), Region::Details, Cell::default())
                    .unwrap()
            )
            .unwrap()
            .text(),
        "<record>source</record>"
    );
    selection.select_all(document);
    assert_eq!(
        selection.text(document).as_deref(),
        Some(
            "Questions answered\n\nQuestion 1\n\nUse café?\n\nAnswer\n\nYes.\n<record>source</record>"
        )
    );
}

#[test]
fn fallback_lines_are_shared_and_keep_their_newlines() {
    let mut transcript = Transcript::new("Test agent");
    transcript.entries.push(agent(1, "ignored"));
    let lines = Rc::from(crate::release_notes::parse("plain\n\n```\nx\ny\n```"));
    let bodies = HashMap::from([(
        EntryId(1),
        (1, Rc::new(Body::Message(vec![Part::Text(lines)]))),
    )]);
    let mut document = Document::default();
    document.update(&transcript, &[(EntryId(1), 1)], &bodies);
    let mut selection = Selection::default();
    selection.select_all(&document);
    assert_eq!(selection.text(&document).as_deref(), Some("plain\n\nx\ny"));
}

#[test]
fn code_projection_obeys_preview_bounds_without_copying_full_source() {
    use crate::chat::{rich::Block, visual::VisualId};
    let source: Arc<str> = format!(
        "{}\n{}",
        "é".repeat(MAX_CODE_LINE_CHARS + 1),
        "x\n".repeat(MAX_CODE_LINES)
    )
    .into();
    let mut transcript = Transcript::new("Test agent");
    transcript.entries.push(agent(1, "ignored"));
    let bodies = HashMap::from([(
        EntryId(1),
        (
            1,
            Rc::new(Body::Message(vec![Part::Rich(
                vec![Block::Code {
                    id: VisualId::of(&source),
                    language: String::new(),
                    source,
                }]
                .into(),
            )])),
        ),
    )]);
    let mut document = Document::default();
    document.update(&transcript, &[(EntryId(1), 1)], &bodies);
    let key = document
        .key(EntryId(1), Region::Body(0), Cell::default())
        .unwrap();
    assert_eq!(
        document.segment(key).unwrap().text().chars().count(),
        MAX_CODE_LINE_CHARS
    );
    assert!(
        document
            .key(EntryId(1), Region::Body(0), Cell::row(MAX_CODE_LINES))
            .is_none()
    );
}

#[test]
fn folded_thoughts_and_tools_never_leak_hidden_text_across_messages() {
    use herdr_chat_protocol::{Tool, ToolContent, ToolKind, ToolStatus};
    let (transcript, mut prepared, keys) = prepare(vec![
        agent(5, "before"),
        Entry {
            id: EntryId(2),
            kind: EntryKind::Thought {
                text: "visible thought\nhidden thought".into(),
            },
        },
        Entry {
            id: EntryId(3),
            kind: EntryKind::Tool(Tool {
                call_id: "test-call".into(),
                title: "Read file".into(),
                kind: ToolKind::Read,
                status: ToolStatus::Completed,
                locations: vec!["src/test.rs".into()],
                content: vec![ToolContent::Text {
                    text: "hidden tool source".into(),
                }],
            }),
        },
        agent(1, "after"),
    ]);
    let mut selection = Selection::default();
    selection.select_all(prepared.text());
    assert_eq!(
        selection.text(prepared.text()).as_deref(),
        Some("before\n\nvisible thought\n\nRead file\n\nafter")
    );
    prepared.set_expanded(&std::collections::HashSet::from([EntryId(2), EntryId(3)]));
    prepared.update(&transcript, &keys);
    assert!(selection.validate(prepared.text()));
    selection.select_all(prepared.text());
    let text = selection.text(prepared.text()).unwrap();
    assert!(text.contains("visible thought\nhidden thought"));
    assert!(text.contains("Read file\nRead file\nsrc/test.rs\nhidden tool source"));
}

#[test]
fn toggling_an_unselected_record_preserves_existing_selection() {
    let (transcript, mut prepared, keys) = prepare(vec![
        agent(1, "stable"),
        Entry {
            id: EntryId(2),
            kind: EntryKind::Notice {
                answers: Vec::new(),
                details: Some("record".into()),
                level: NoticeLevel::Info,
                text: "label".into(),
            },
        },
    ]);
    let mut selection = Selection::default();
    let start = caret(prepared.text(), 1, 0, 0, 0, 0);
    selection.press(prepared.text(), start, 3, false);
    prepared.set_expanded(&std::collections::HashSet::from([EntryId(2)]));
    prepared.update(&transcript, &keys);
    assert!(!selection.validate(prepared.text()));
    assert_eq!(selection.text(prepared.text()).as_deref(), Some("stable"));
}

#[test]
fn oversized_selection_is_refused_instead_of_silently_truncated() {
    let mut transcript = Transcript::new("Test agent");
    let entry_bytes = 256 * 1024;
    transcript.entries = (0..=crate::terminal::MAX_SELECTION_BYTES / entry_bytes)
        .map(|id| Entry {
            id: EntryId(id as u64),
            kind: EntryKind::Notice {
                answers: Vec::new(),
                details: None,
                level: NoticeLevel::Info,
                text: "x".repeat(entry_bytes),
            },
        })
        .collect();
    let keys = transcript
        .entries
        .iter()
        .map(|entry| (entry.id, 1))
        .collect::<Vec<_>>();
    let mut document = Document::default();
    document.update(&transcript, &keys, &HashMap::new());
    let mut selection = Selection::default();
    selection.select_all(&document);
    assert!(selection.text(&document).is_none());
}

#[test]
fn ranges_never_include_hidden_preview_or_widget_sources() {
    let (_, prepared, _) = prepare(vec![agent(
        1,
        concat!(
            "before\n\n",
            "```html\n<p>hidden HTML</p>\n```\n\n",
            "```mermaid\ngraph LR; A-->B\n```\n\n",
            "```herdr-ui\n{\"components\":[{\"type\":\"text\",\"text\":\"card\"}]}\n```\n\n",
            "after",
        ),
    )]);
    let mut selection = Selection::default();
    selection.select_all(prepared.text());
    assert_eq!(
        selection.text(prepared.text()).as_deref(),
        Some("before\n\nafter")
    );
}

#[test]
fn empty_code_ranges_with_removed_controls_do_not_copy_or_panic() {
    let (_, prepared, _) = prepare(vec![agent(1, "before\n\n```text\n\u{1b}a\u{1b}\n```")]);
    let document = prepared.text();
    let code = caret(document, 1, 1, 0, 0, 0);
    let segment = document.segment(code.key).unwrap();
    assert_eq!(segment.text(), "a");
    assert_eq!(segment.copy(0..0), "");
    assert_eq!(segment.copy(1..1), "");
    let mut selection = Selection::default();
    selection.press(document, caret(document, 1, 0, 0, 0, 0), 1, false);
    selection.drag(document, code);
    assert_eq!(selection.text(document).as_deref(), Some("before\n\n"));
}
