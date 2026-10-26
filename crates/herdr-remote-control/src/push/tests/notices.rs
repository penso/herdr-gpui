use super::*;
use herdr_chat_protocol::{
    Change, Entry, EntryId, EntryKind, NoticeLevel, QuestionAnswer, Transcript,
};

#[test]
fn rich_notice_metadata_never_becomes_a_push_excerpt_or_live_question() {
    let chat = Chat {
        agent: "Claude",
        pane: "w1-p1",
        title: "review",
        workspace: "example",
    };
    let before = Transcript::new("Claude");
    for level in [NoticeLevel::Info, NoticeLevel::Warning, NoticeLevel::Error] {
        let change = Change::Push {
            entry: Entry {
                id: EntryId(1),
                kind: EntryKind::Notice {
                    answers: vec![QuestionAnswer {
                        question: "Historical question".into(),
                        answer: "Private answer".into(),
                    }],
                    details: Some("Private original record".into()),
                    level,
                    text: "Summary".into(),
                },
            },
        };
        for privacy in [Privacy::Generic, Privacy::Excerpt] {
            let notification = decide(&chat, &before, &change, privacy);
            if level == NoticeLevel::Error {
                let notification = notification.unwrap();
                assert_eq!(notification.kind, Kind::Error);
                assert_eq!(
                    notification.body,
                    if privacy == Privacy::Excerpt {
                        "Summary\nexample · review"
                    } else {
                        "example · review"
                    }
                );
            } else {
                assert_eq!(notification, None);
            }
        }
    }
}
