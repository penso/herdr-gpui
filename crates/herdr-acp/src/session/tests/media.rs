use super::*;
use herdr_chat_protocol::MAX_IMAGE_DATA;

fn picture(harness: &mut Harness, index: usize) {
    let data = format!("{index:04}{}", "A".repeat(MAX_IMAGE_DATA - 4));
    harness.update(json!({"sessionUpdate": "agent_message_chunk",
        "content": {"type": "image", "data": data, "mimeType": "image/png"}}));
}

#[test]
fn pictures_have_a_budget_of_their_own() {
    let mut harness = ready();
    let id = prompt(&mut harness, 0, "go");
    let pictures = MAX_TRANSCRIPT_MEDIA / MAX_IMAGE_DATA + 3;
    for index in 0..pictures {
        picture(&mut harness, index);
    }
    harness.respond(id, json!({"stopReason": "end_turn"}));
    let transcript = harness.transcript();
    let media: usize = transcript.entries.iter().map(entry_media).sum();
    assert!(media <= MAX_TRANSCRIPT_MEDIA, "{media}");
    assert!(transcript.truncated);
    // The newest picture stays; the text budget is untouched by them.
    let newest = format!("{:04}", pictures - 1);
    assert!(transcript.entries.iter().any(|entry| matches!(&entry.kind,
        EntryKind::Image { image, .. } if image.data.starts_with(&newest))));
    let text: usize = transcript.entries.iter().map(entry_size).sum();
    assert!(text <= MAX_TRANSCRIPT_TEXT);
}

#[test]
fn the_reconnect_log_is_bounded_in_bytes() {
    let mut harness = ready();
    let id = prompt(&mut harness, 0, "go");
    for index in 0..LOG_BYTES / MAX_IMAGE_DATA + 3 {
        picture(&mut harness, index);
    }
    harness.respond(id, json!({"stopReason": "end_turn"}));
    let session = &harness.session;
    assert!(session.log_bytes <= LOG_BYTES, "{}", session.log_bytes);
    assert_eq!(
        session.log_bytes,
        session
            .log
            .iter()
            .map(|(_, change)| change_size(change))
            .sum::<usize>()
    );
    assert!(!session.log.is_empty());
}

#[test]
fn question_cards_count_parsed_text_and_original_source_in_retention() {
    let entry = herdr_chat_protocol::Entry {
        id: herdr_chat_protocol::EntryId(1),
        kind: EntryKind::Notice {
            answers: vec![herdr_chat_protocol::QuestionAnswer {
                question: "Which approach?".into(),
                answer: "A custom answer".into(),
            }],
            details: Some("Original source".into()),
            level: NoticeLevel::Info,
            text: "Question answered".into(),
        },
    };
    let size = "Which approach?".len()
        + "A custom answer".len()
        + "Original source".len()
        + "Question answered".len();
    assert_eq!(entry_size(&entry), size);
    assert_eq!(change_size(&Change::Push { entry }), size);
}
