use super::*;
use crate::chat::prepared::{Body, Part, Prepared};
use core::prelude::v1::test;
use herdr_chat_protocol::{Entry, EntryId, EntryKind, Transcript};

const KEY: (EntryId, usize) = (EntryId(4), 0);

fn update(prepared: &mut Prepared, state: &mut State, text: Option<&str>, revision: u64) {
    let mut transcript = Transcript::new("Test");
    let mut keys = Vec::new();
    if let Some(text) = text {
        transcript.entries.push(Entry {
            id: KEY.0,
            kind: EntryKind::Agent { text: text.into() },
        });
        keys.push((KEY.0, revision));
    }
    prepared.update(&transcript, &keys);
    state.retain(prepared);
}

fn document(prepared: &Prepared) -> Rc<Document> {
    let body = prepared.body(KEY.0).unwrap();
    let Body::Message(parts) = body.as_ref() else {
        panic!("message");
    };
    let Part::Widget(document) = &parts[KEY.1] else {
        panic!("widget");
    };
    document.clone()
}

#[test]
fn replaced_source_cannot_restore_old_values_without_editing_its_replacement() {
    let original = format!("```herdr-ui\n{FORM}\n```");
    let replacement = original.replace("initial", "revised");
    let mut prepared = Prepared::default();
    let mut state = State::default();
    update(&mut prepared, &mut state, Some(&original), 1);
    let first = document(&prepared);
    first.set(
        &mut state.edit(KEY, &first).values,
        "note",
        Value::Text("edited".into()),
    );

    update(&mut prepared, &mut state, Some(&replacement), 2);
    assert!(state.form(KEY, &document(&prepared)).is_none());
    update(&mut prepared, &mut state, Some(&original), 3);
    let restored = document(&prepared);
    assert!(state.form(KEY, &restored).is_none());
    assert_eq!(
        state.edit(KEY, &restored).values["note"],
        Value::Text("initial".into())
    );
}

#[test]
fn removed_unfinished_or_moved_cards_cannot_revive_old_values() {
    let original = format!("```herdr-ui\n{FORM}\n```");
    let unfinished = format!("```herdr-ui\n{FORM}\n");
    let moved = format!("New introduction\n\n{original}");
    for replacement in [None, Some("Plain text"), Some(&unfinished), Some(&moved)] {
        let mut prepared = Prepared::default();
        let mut state = State::default();
        update(&mut prepared, &mut state, Some(&original), 1);
        let first = document(&prepared);
        first.set(
            &mut state.edit(KEY, &first).values,
            "note",
            Value::Text("edited".into()),
        );
        update(&mut prepared, &mut state, replacement, 2);
        update(&mut prepared, &mut state, Some(&original), 3);
        assert!(state.form(KEY, &document(&prepared)).is_none());
    }
}

#[test]
fn unchanged_card_keeps_values_when_surrounding_message_text_changes() {
    let original = format!("```herdr-ui\n{FORM}\n```\n\nFirst explanation");
    let revised = original.replace("First explanation", "More streamed explanation");
    let mut prepared = Prepared::default();
    let mut state = State::default();
    update(&mut prepared, &mut state, Some(&original), 1);
    let first = document(&prepared);
    first.set(
        &mut state.edit(KEY, &first).values,
        "note",
        Value::Text("edited".into()),
    );
    update(&mut prepared, &mut state, Some(&revised), 2);
    let current = document(&prepared);
    assert!(!Rc::ptr_eq(&first, &current));
    let form = state.form(KEY, &current).unwrap();
    assert!(Rc::ptr_eq(&form.document, &first));
    assert_eq!(form.values["note"], Value::Text("edited".into()));
}
