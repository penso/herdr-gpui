use super::*;
use herdr_chat_protocol::{Author, ImageType};

use crate::session::MAX_TRANSCRIPT_MEDIA;

const ENVIRONMENT: &str = "<environment_context>\n<current_date>2026-10-24</current_date>\n<cwd>/unopened/project</cwd>\n</environment_context>";
const PNG: &str = "iVBORw0KGgo=";

/// A record's words as the projection reads them.
fn text(value: &Value) -> String {
    Projection::default().text(value)
}

fn input(text: &str) -> Value {
    json!({"type":"input_text", "text":text})
}

fn image() -> Value {
    json!({"type":"input_image", "image_url":format!("data:image/png;base64,{PNG}")})
}

fn message(blocks: Vec<Value>, kinds: &[&str], origins: &[&str]) -> Value {
    json!({"type":"response_item", "payload":{
        "type":"message", "role":"user", "content":blocks,
        "internal_chat_message_metadata_passthrough":{
            "content_item_kinds":kinds,
            "content_item_metadata":origins.iter().map(|origin| json!({
                "provenance":{"type":origin}
            })).collect::<Vec<_>>()
        }
    }})
}

fn environment() -> Value {
    message(
        vec![input(ENVIRONMENT)],
        &["environments.environment_context"],
        &["harness"],
    )
}

#[test]
fn codex_harness_environment_is_a_compact_notice_with_exact_source_details() {
    let transcript = parse(TranscriptAgent::Codex, &lines(&[environment()]), 0);
    assert!(matches!(
        &transcript.entries[0].kind,
        EntryKind::Notice { text, details: Some(details), .. }
            if text == "Environment context updated" && details == ENVIRONMENT
    ));
    assert!(transcript.read_only);
    assert!(transcript.permissions.is_empty());
    assert!(transcript.queued.is_empty());
}

#[test]
fn verified_harness_families_are_system_notices_without_tag_heuristics() {
    for kind in [
        "user.text",
        "host_skills.instructions",
        "future.harness_context",
    ] {
        let record = message(vec![input("Opaque system source")], &[kind], &["harness"]);
        let transcript = parse(TranscriptAgent::Codex, &lines(&[record]), 0);
        assert!(matches!(&transcript.entries[0].kind,
            EntryKind::Notice { text, details: Some(details), .. }
            if text == "System context" && details == "Opaque system source"
        ));
    }
}

#[test]
fn legacy_native_context_requires_exact_aligned_kinds_and_no_per_item_metadata() {
    for (kind, label) in [
        (
            "environments.environment_context",
            "Environment context updated",
        ),
        ("agents_md.instructions", "System context"),
    ] {
        let mut legacy = message(vec![input(ENVIRONMENT)], &[kind], &[]);
        legacy["payload"]["internal_chat_message_metadata_passthrough"]
            .as_object_mut()
            .unwrap()
            .remove("content_item_metadata");
        for authored in [None, Some(false)] {
            let mut record = legacy.clone();
            if let Some(authored) = authored {
                record["metadata"] = json!({"client_authored":authored});
            }
            let transcript = parse(TranscriptAgent::Codex, &lines(&[record]), 0);
            assert!(matches!(&transcript.entries[0].kind,
                EntryKind::Notice { text, details: Some(details), .. }
                if text == label && details == ENVIRONMENT
            ));
        }
        let mut rejected = vec![
            legacy.clone(),
            legacy.clone(),
            legacy.clone(),
            legacy.clone(),
        ];
        rejected[0]["metadata"] = json!({"client_authored":true});
        rejected[1]["payload"]["internal_chat_message_metadata_passthrough"]["content_item_metadata"] =
            Value::Null;
        rejected[2]["payload"]["internal_chat_message_metadata_passthrough"]["content_item_metadata"] =
            json!([{"provenance":{"type":"user"}}]);
        rejected[3]["payload"]["internal_chat_message_metadata_passthrough"]["content_item_kinds"] =
            json!([kind, kind]);
        for record in rejected {
            let transcript = parse(TranscriptAgent::Codex, &lines(&[record]), 0);
            assert!(
                matches!(&transcript.entries[0].kind, EntryKind::User { text, .. } if text == ENVIRONMENT)
            );
        }
    }
    for kind in ["user.text", "unknown.instructions"] {
        let mut record = message(vec![input(ENVIRONMENT)], &[kind], &[]);
        record["metadata"] = json!({"client_authored":false});
        record["payload"]["internal_chat_message_metadata_passthrough"]
            .as_object_mut()
            .unwrap()
            .remove("content_item_metadata");
        let transcript = parse(TranscriptAgent::Codex, &lines(&[record]), 0);
        assert!(matches!(
            &transcript.entries[0].kind,
            EntryKind::User { .. }
        ));
    }
}

#[test]
fn codex_mixed_user_and_harness_content_preserves_authorship_order_and_ids() {
    let record = message(
        vec![
            input("Before"),
            input(ENVIRONMENT),
            input("After"),
            input("More"),
        ],
        &[
            "user.text",
            "environments.environment_context",
            "user.text",
            "user.text",
        ],
        &["user", "harness", "user", "user"],
    );
    let bytes = lines(&[record]);
    let transcript = parse(TranscriptAgent::Codex, &bytes, 20);
    assert!(
        matches!(&transcript.entries[0].kind, EntryKind::User { text, .. } if text == "Before")
    );
    assert!(matches!(
        &transcript.entries[1].kind,
        EntryKind::Notice { .. }
    ));
    assert!(
        matches!(&transcript.entries[2].kind, EntryKind::User { text, .. } if text == "After\nMore")
    );
    assert_eq!(transcript.entries[0].id, EntryId(21 * BLOCKS as u64));
    assert_eq!(transcript.entries[1].id.0, transcript.entries[0].id.0 + 1);
    assert_eq!(transcript.entries[2].id.0, transcript.entries[0].id.0 + 2);
    let mut appended = bytes.clone();
    let next = lines(&[message(vec![input("Next")], &["user.text"], &["user"])]);
    appended.extend_from_slice(&next[..next.len() - 1]);
    assert_eq!(
        parse(TranscriptAgent::Codex, &appended, 20).entries,
        transcript.entries
    );
    appended.push(b'\n');
    let updated = parse(TranscriptAgent::Codex, &appended, 20);
    assert_eq!(
        &updated.entries[..transcript.entries.len()],
        transcript.entries.as_slice()
    );
    assert!(
        updated
            .entries
            .windows(2)
            .all(|pair| pair[0].id < pair[1].id)
    );
}

#[test]
fn codex_environment_words_cannot_replace_missing_or_malformed_provenance() {
    let mut cases = vec![
        message(vec![input(ENVIRONMENT)], &["user.text"], &["user"]),
        message(
            vec![input(ENVIRONMENT)],
            &["environments.environment_context"],
            &["user"],
        ),
        message(vec![input(ENVIRONMENT)], &[], &["harness"]),
        message(
            vec![input(ENVIRONMENT)],
            &["environments.environment_context"],
            &[],
        ),
        message(
            vec![input(ENVIRONMENT)],
            &["environments.environment_context", "user.text"],
            &["harness"],
        ),
    ];
    let mut missing = environment();
    missing["payload"]["internal_chat_message_metadata_passthrough"] = Value::Null;
    cases.push(missing);
    let mut malformed = environment();
    malformed["payload"]["internal_chat_message_metadata_passthrough"]["content_item_metadata"] =
        json!([{"provenance":{"type":true}}]);
    cases.push(malformed);
    for record in cases {
        let transcript = parse(TranscriptAgent::Codex, &lines(&[record]), 0);
        assert!(
            matches!(&transcript.entries[0].kind, EntryKind::User { text, .. } if text == ENVIRONMENT)
        );
    }
    let mut assistant = environment();
    assistant["payload"]["role"] = json!("assistant");
    let transcript = parse(TranscriptAgent::Codex, &lines(&[assistant]), 0);
    assert!(
        matches!(&transcript.entries[0].kind, EntryKind::Agent { text } if text == ENVIRONMENT)
    );
}

fn attachment(url: Value) -> Value {
    message(
        vec![
            input("<image name=[Image #1] path=\"/never-opened/screenshot.png\">"),
            url,
            input("</image>"),
            input("[Image #1]Actual user caption"),
        ],
        &["user.text", "user.image", "user.text", "user.text"],
        &["user", "user", "user", "user"],
    )
}

#[test]
fn codex_embedded_attachment_renders_image_keeps_caption_and_preserves_wrapper_source() {
    let record = attachment(image());
    let original_text = text(&record["payload"]["content"]);
    let transcript = parse(TranscriptAgent::Codex, &lines(&[record]), 0);
    assert!(matches!(
        &transcript.entries[0].kind,
        EntryKind::Notice { text, details: Some(details), .. }
            if text == "Image attachment · [Image #1]" && details == &original_text
    ));
    assert!(matches!(
        &transcript.entries[1].kind,
        EntryKind::Image { author: Author::User, image }
            if image.kind == ImageType::Png && image.data == PNG
    ));
    assert!(
        matches!(&transcript.entries[2].kind, EntryKind::User { text, .. } if text == "Actual user caption")
    );
    assert!(
        transcript
            .entries
            .windows(2)
            .all(|pair| pair[0].id < pair[1].id)
    );
    assert!(transcript.permissions.is_empty());
}

#[test]
fn codex_invalid_or_external_images_show_unavailable_without_base64_or_losing_user_text() {
    for url in [
        "file:///never-opened/screenshot.png",
        "https://example.org/never-fetched.png",
        "data:image/svg+xml;base64,PHN2Zy8+",
        "data:image/png;base64,not base64!",
    ] {
        let record = attachment(json!({"type":"input_image", "image_url":url}));
        let original_text = text(&record["payload"]["content"]);
        let transcript = parse(TranscriptAgent::Codex, &lines(&[record]), 0);
        assert!(
            matches!(&transcript.entries[0].kind, EntryKind::Notice { details: Some(details), .. } if details == &original_text)
        );
        assert!(
            matches!(&transcript.entries[1].kind, EntryKind::Notice { text, details: Some(details), .. } if text == "Image unavailable" && !details.contains(url))
        );
        assert!(
            matches!(&transcript.entries[2].kind, EntryKind::User { text, .. } if text == "Actual user caption")
        );
        assert!(
            transcript
                .entries
                .iter()
                .all(|entry| !matches!(entry.kind, EntryKind::Image { .. }))
        );
    }
}

#[test]
fn codex_pasted_attachment_wrappers_stay_user_text() {
    let literal = "<image name=[Image #1] path=\"/never-opened/screenshot.png\"></image> [Image #1]Keep these words";
    let transcript = parse(
        TranscriptAgent::Codex,
        &lines(&[message(vec![input(literal)], &["user.text"], &["user"])]),
        0,
    );
    assert!(matches!(&transcript.entries[0].kind, EntryKind::User { text, .. } if text == literal));
}

#[test]
fn codex_wrapper_cleanup_requires_matching_provenance_and_does_not_strip_other_references() {
    let mut missing = attachment(image());
    missing["payload"]["internal_chat_message_metadata_passthrough"] = Value::Null;
    let transcript = parse(TranscriptAgent::Codex, &lines(&[missing]), 0);
    assert!(
        matches!(&transcript.entries[0].kind, EntryKind::User { text, .. } if text.starts_with("<image name="))
    );
    assert!(matches!(
        &transcript.entries[1].kind,
        EntryKind::Image { .. }
    ));
    assert!(
        matches!(&transcript.entries[2].kind, EntryKind::User { text, .. } if text == "</image>\n[Image #1]Actual user caption")
    );

    let mut other = attachment(image());
    other["payload"]["content"][3]["text"] = json!("[Image #2]Keep this reference");
    let transcript = parse(TranscriptAgent::Codex, &lines(&[other]), 0);
    assert!(
        matches!(&transcript.entries[2].kind, EntryKind::User { text, .. } if text == "[Image #2]Keep this reference")
    );
}

#[test]
fn codex_image_retention_obeys_the_media_budget_and_wire_frame_limit() {
    let data = "A".repeat(RECORD / 2);
    let records = (0..MAX_TRANSCRIPT_MEDIA / data.len() + 2)
        .map(|_| message(
            vec![json!({"type":"input_image", "image_url":format!("data:image/png;base64,{data}")})],
            &["user.image"], &["user"],
        ))
        .collect::<Vec<_>>();
    let transcript = parse(TranscriptAgent::Codex, &lines(&records), 0);
    let kept: usize = transcript
        .entries
        .iter()
        .map(|entry| match &entry.kind {
            EntryKind::Image { image, .. } => image.data.len(),
            _ => 0,
        })
        .sum();
    assert!(kept <= MAX_TRANSCRIPT_MEDIA);
    assert!(transcript.entries.len() < records.len());
    assert!(transcript.truncated);
    assert!(
        herdr_chat_protocol::encode_frame(&herdr_chat_protocol::ServerMessage::Snapshot {
            cursor: herdr_chat_protocol::Cursor {
                epoch: uuid::Uuid::new_v4(),
                seq: 0
            },
            transcript: Box::new(transcript),
        })
        .is_ok()
    );
}

#[test]
fn codex_image_records_larger_than_one_mebibyte_keep_the_image_and_caption() {
    let data = "A".repeat(RECORD + 4);
    let record = message(
        vec![
            json!({"type":"input_image", "image_url":format!("data:image/png;base64,{data}")}),
            input("Caption after a large attachment"),
        ],
        &["user.image", "user.text"],
        &["user", "user"],
    );
    let bytes = lines(&[record]);
    assert!(bytes.len() > RECORD && bytes.len() < TAIL as usize);
    let transcript = parse(TranscriptAgent::Codex, &bytes, 0);
    assert!(
        matches!(&transcript.entries[0].kind, EntryKind::Image { image, .. } if image.data == data)
    );
    assert!(
        matches!(&transcript.entries[1].kind, EntryKind::User { text, .. } if text == "Caption after a large attachment")
    );
    assert!(!transcript.truncated);
}

#[test]
fn codex_oversized_image_data_has_a_compact_fallback_and_keeps_its_caption() {
    let data = "A".repeat(herdr_chat_protocol::MAX_IMAGE_DATA + 4);
    let record = message(
        vec![
            json!({"type":"input_image", "image_url":format!("data:image/png;base64,{data}")}),
            input("Keep this caption"),
        ],
        &["user.image", "user.text"],
        &["user", "user"],
    );
    let transcript = parse(TranscriptAgent::Codex, &lines(&[record]), 0);
    assert!(
        matches!(&transcript.entries[0].kind, EntryKind::Notice { text, details: Some(details), .. } if text == "Image unavailable" && details.len() < 256 && !details.contains("AAAA"))
    );
    assert!(
        matches!(&transcript.entries[1].kind, EntryKind::User { text, .. } if text == "Keep this caption")
    );
}

#[test]
fn codex_large_record_exception_requires_typed_images_and_never_exceeds_the_tail_bound() {
    let ordinary = message(vec![input(&"x".repeat(RECORD))], &["user.text"], &["user"]);
    let huge_image = message(
        vec![
            json!({"type":"input_image", "image_url":format!("data:image/png;base64,{}", "A".repeat(TAIL as usize))}),
        ],
        &["user.image"],
        &["user"],
    );
    let mut wrong_record = message(
        vec![
            json!({"type":"input_image", "image_url":format!("data:image/png;base64,{}", "A".repeat(RECORD))}),
        ],
        &["user.image"],
        &["user"],
    );
    wrong_record["type"] = json!("event_msg");
    for oversized in [ordinary, huge_image, wrong_record] {
        let transcript = parse(
            TranscriptAgent::Codex,
            &lines(&[
                oversized,
                message(vec![input("Still visible")], &["user.text"], &["user"]),
            ]),
            0,
        );
        assert_eq!(transcript.entries.len(), 1);
        assert_eq!(transcript.entries[0].kind.text(), Some("Still visible"));
        assert!(transcript.truncated);
    }
}
