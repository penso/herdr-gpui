use super::*;
use serde_json::json;

#[test]
fn ordinary_notices_keep_their_existing_wire_shape() {
    let value = json!({"type":"notice", "level":"info", "text":"Connected"});
    let notice: EntryKind = serde_json::from_value(value.clone()).unwrap();
    assert_eq!(
        notice,
        EntryKind::Notice {
            answers: Vec::new(),
            details: None,
            level: NoticeLevel::Info,
            text: "Connected".into(),
        }
    );
    assert_eq!(serde_json::to_value(&notice).unwrap(), value);
}

#[test]
fn notice_source_round_trips_and_older_clients_can_still_read_its_summary() {
    #[derive(Debug, PartialEq, serde::Deserialize)]
    #[serde(tag = "type", rename_all = "snake_case")]
    enum OlderEntry {
        Notice { level: NoticeLevel, text: String },
    }

    let notice = EntryKind::Notice {
        answers: vec![QuestionAnswer {
            question: "Which method?".into(),
            answer: "A custom answer".into(),
        }],
        details: Some("<task-notification>\nraw source\n</task-notification>".into()),
        level: NoticeLevel::Info,
        text: "Task notification · Check complete".into(),
    };
    let value = serde_json::to_value(&notice).unwrap();
    assert_eq!(
        serde_json::from_value::<EntryKind>(value.clone()).unwrap(),
        notice
    );
    assert_eq!(
        serde_json::from_value::<OlderEntry>(value).unwrap(),
        OlderEntry::Notice {
            level: NoticeLevel::Info,
            text: "Task notification · Check complete".into(),
        }
    );
}

fn answered(answers: Vec<QuestionAnswer>) -> EntryKind {
    EntryKind::Notice {
        answers,
        details: None,
        level: NoticeLevel::Info,
        text: "Question answered".into(),
    }
}

#[test]
fn question_answer_wire_limits_reject_count_and_text_overflow() {
    for answers in [
        vec![
            QuestionAnswer {
                question: "Q".into(),
                answer: "A".into()
            };
            MAX_QUESTION_ANSWERS + 1
        ],
        vec![QuestionAnswer {
            question: "Q".into(),
            answer: "α".repeat(MAX_QUESTION_TEXT / 2),
        }],
    ] {
        assert!(!valid_question_answers(&answers));
        assert!(serde_json::to_vec(&answered(answers.clone())).is_err());
        let value =
            json!({"type":"notice", "level":"info", "text":"Question answered", "answers":answers});
        assert!(serde_json::from_value::<EntryKind>(value).is_err());
    }
    let edge = answered(vec![QuestionAnswer {
        question: "Q".into(),
        answer: "A".repeat(MAX_QUESTION_TEXT - 1),
    }]);
    let bytes = serde_json::to_vec(&edge).unwrap();
    assert_eq!(serde_json::from_slice::<EntryKind>(&bytes).unwrap(), edge);
    let edge = answered(vec![
        QuestionAnswer {
            question: "Q".into(),
            answer: "A".into()
        };
        MAX_QUESTION_ANSWERS
    ]);
    let bytes = serde_json::to_vec(&edge).unwrap();
    assert_eq!(serde_json::from_slice::<EntryKind>(&bytes).unwrap(), edge);
}

#[test]
fn direct_question_answer_changes_validate_before_mutating_the_transcript() {
    let original = agent(1, "Original");
    let mut transcript = Transcript {
        entries: vec![original],
        ..Transcript::default()
    };
    let before = transcript.clone();
    let oversized = answered(vec![
        QuestionAnswer {
            question: "Q".into(),
            answer: "A".into()
        };
        MAX_QUESTION_ANSWERS + 1
    ]);
    for change in [
        Change::Replace {
            entry: Entry {
                id: EntryId(1),
                kind: oversized.clone(),
            },
        },
        Change::Push {
            entry: Entry {
                id: EntryId(2),
                kind: oversized,
            },
        },
    ] {
        assert!(matches!(
            transcript.apply(&change),
            Err(Error::QuestionAnswersLimit(_))
        ));
        assert_eq!(transcript, before);
    }
}
