//! A language switch prepares the chat's labels again: a question's heading
//! and the selectable text drawn from it follow the locale now shown.

use super::*;
use crate::chat::prepared::{Body, Prepared};
use herdr_chat_protocol::{NoticeLevel, QuestionAnswer};
use herdr_i18n::{Locale, Pseudo};

fn answered() -> Transcript {
    let mut transcript = Transcript::new("Codex");
    transcript.entries.push(Entry {
        id: EntryId(1),
        kind: EntryKind::Notice {
            answers: vec![QuestionAnswer {
                question: "Which file?".into(),
                answer: "main.rs".into(),
            }],
            details: None,
            level: NoticeLevel::Info,
            text: "Questions answered".into(),
        },
    });
    transcript
}

fn answers(prepared: &Prepared) -> String {
    match prepared.body(EntryId(1)).as_deref() {
        Some(Body::Answers { text, .. }) => text.to_string(),
        _ => panic!("prepared answers"),
    }
}

#[test]
fn forgetting_the_text_writes_its_labels_in_the_locale_now_shown() {
    crate::i18n::show(&Locale::FALLBACK);
    let transcript = answered();
    let keys = [(EntryId(1), 1)];
    let mut prepared = Prepared::default();
    prepared.update(&transcript, &keys);
    let english = answers(&prepared);
    assert!(english.contains("**Question "), "{english}");

    crate::i18n::show(&Pseudo::Accented.locale());
    prepared.update(&transcript, &keys);
    assert_eq!(answers(&prepared), english, "same revision, same body");
    prepared.forget_text();
    prepared.update(&transcript, &keys);
    let accented = answers(&prepared);
    assert_ne!(accented, english);
    assert!(!accented.contains("**Question "), "{accented}");
    crate::i18n::show(&Locale::FALLBACK);
}

#[test]
fn a_new_locale_prepares_the_panes_again_once() {
    let mut chats = Chats::default();
    let english = Some(Locale::FALLBACK);
    assert!(!chats.relocale(english.clone()), "nothing drawn before");
    assert!(!chats.relocale(english.clone()));
    assert!(chats.relocale(Some(Pseudo::Accented.locale())));
    assert!(!chats.relocale(Some(Pseudo::Accented.locale())));
    assert!(chats.relocale(english));
}
