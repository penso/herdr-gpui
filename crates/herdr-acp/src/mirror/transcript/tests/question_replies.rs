use super::*;
use herdr_chat_protocol::{MAX_QUESTION_ANSWERS, QuestionAnswer};

const TOOL: &str = "request_user_input_async";

mod retained;

fn request(call: &str, questions: &[&str]) -> Value {
    json!({"type":"response_item", "payload":{
        "type":"function_call", "name":TOOL, "call_id":call,
        "arguments":json!({"questions":questions.iter().map(|title| json!({
            "title":title, "options":["First", "Second"]
        })).collect::<Vec<_>>()}).to_string()
    }})
}

fn reply(call: &str, index: usize, question: &str, answer: &str) -> Value {
    json!({"questionItemId":json!([TOOL,call,index]).to_string(), "question":question, "answer":answer})
}

fn envelope(replies: Vec<Value>) -> String {
    format!(
        "<send_user_message_question_reply>{}</send_user_message_question_reply>",
        json!(replies)
    )
}

fn message(blocks: &[&str]) -> Value {
    json!({"type":"response_item", "metadata":{"client_authored":false}, "payload":{
        "type":"message", "role":"user",
        "content":blocks.iter().map(|text| json!({"type":"input_text", "text":text})).collect::<Vec<_>>(),
        "internal_chat_message_metadata_passthrough":{"content_item_kinds":vec!["user.text";blocks.len()]}
    }})
}

#[test]
fn exact_async_reply_preserves_free_text_and_source_without_replaying_questions() {
    let source = envelope(vec![reply(
        "call1",
        0,
        "Which approach?",
        "A free-text alternative",
    )]);
    let transcript = parse(
        TranscriptAgent::Codex,
        &lines(&[request("call1", &["Which approach?"]), message(&[&source])]),
        0,
    );
    assert!(matches!(&transcript.entries[1].kind,
        EntryKind::Notice { answers, details: Some(details), text, .. }
        if text == "Question answered" && details == &source && answers == &vec![QuestionAnswer {
            question:"Which approach?".into(), answer:"A free-text alternative".into(),
        }]
    ));
    assert!(transcript.read_only);
    assert!(transcript.permissions.is_empty());
    assert!(transcript.queued.is_empty());
    let EntryKind::Tool(tool) = &transcript.entries[0].kind else {
        panic!("request")
    };
    assert_eq!(tool.title, TOOL);
    assert_eq!(tool.call_id, "call1");
    assert_eq!(tool.status, ToolStatus::Pending);
}

#[test]
fn multiple_answers_and_mixed_user_blocks_preserve_order_and_ids() {
    let source = envelope(vec![
        reply("call1", 1, "Second question?", "β"),
        reply("call1", 0, "First question?", "α"),
    ]);
    let transcript = parse(
        TranscriptAgent::Codex,
        &lines(&[
            request("call1", &["First question?", "Second question?"]),
            message(&["Before", &source, "After"]),
        ]),
        0,
    );
    assert!(
        matches!(&transcript.entries[1].kind, EntryKind::User { text, .. } if text == "Before")
    );
    assert!(matches!(&transcript.entries[2].kind,
        EntryKind::Notice { answers, text, .. } if text == "Questions answered" && answers.iter().map(|a| a.answer.as_str()).eq(["β", "α"])
    ));
    assert!(matches!(&transcript.entries[3].kind, EntryKind::User { text, .. } if text == "After"));
    assert_eq!(transcript.entries[2].id.0, transcript.entries[1].id.0 + 1);
    assert_eq!(transcript.entries[3].id.0, transcript.entries[1].id.0 + 2);
}

#[test]
fn unmatched_or_unverified_replies_stay_literal_user_text() {
    let valid = envelope(vec![reply("call1", 0, "Which approach?", "First")]);
    let mut records = Vec::new();
    for authored in [Value::Null, json!(true), json!("false")] {
        let mut record = message(&[&valid]);
        record["metadata"]["client_authored"] = authored;
        records.push((request("call1", &["Which approach?"]), record));
    }
    for source in [
        envelope(vec![reply("missing", 0, "Which approach?", "First")]),
        envelope(vec![reply("call1", 1, "Which approach?", "First")]),
        envelope(vec![reply("call1", 0, "Other question?", "First")]),
        envelope(vec![
            json!({"question":"Which approach?", "answer":"First"}),
        ]),
        envelope(vec![
            json!({"question":"Which approach?", "answer":true, "questionItemId":json!([TOOL,"call1",0]).to_string()}),
        ]),
        envelope(Vec::new()),
        "<send_user_message_question_reply>not JSON</send_user_message_question_reply>".into(),
        format!("Quoted: {valid}"),
        format!("```xml\n{valid}\n```"),
    ] {
        records.push((request("call1", &["Which approach?"]), message(&[&source])));
    }
    let mut wrong_tool = request("call1", &["Which approach?"]);
    wrong_tool["payload"]["name"] = json!("request_user_input");
    records.push((wrong_tool, message(&[&valid])));
    let mut wrong_schema = request("call1", &["Which approach?"]);
    wrong_schema["payload"]["arguments"] =
        json!("{\"questions\":[{\"question\":\"Which approach?\"}]}");
    records.push((wrong_schema, message(&[&valid])));
    for (request, record) in records {
        let source = record["payload"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .to_owned();
        let transcript = parse(TranscriptAgent::Codex, &lines(&[request, record]), 0);
        assert!(
            matches!(&transcript.entries[1].kind, EntryKind::User { text, .. } if text == &source)
        );
    }
    let transcript = parse(TranscriptAgent::Codex, &lines(&[message(&[&valid])]), 0);
    assert!(matches!(
        &transcript.entries[0].kind,
        EntryKind::User { .. }
    ));
}

#[test]
fn malformed_batches_are_atomic_and_answered_ids_are_consumed() {
    let first = reply("call1", 0, "Question?", "Answer");
    let valid = envelope(vec![first.clone()]);
    for invalid in [
        envelope(vec![first.clone(), first.clone()]),
        envelope(vec![
            first.clone(),
            reply("call1", 1, "Wrong question", "Other"),
        ]),
    ] {
        let transcript = parse(
            TranscriptAgent::Codex,
            &lines(&[
                request("call1", &["Question?", "Another question?"]),
                message(&[&invalid]),
                message(&[&valid]),
                message(&[&valid]),
            ]),
            0,
        );
        assert!(matches!(
            &transcript.entries[1].kind,
            EntryKind::User { .. }
        ));
        assert!(matches!(
            &transcript.entries[2].kind,
            EntryKind::Notice { .. }
        ));
        assert!(matches!(&transcript.entries[3].kind,
            EntryKind::Notice { answers, details: Some(details), .. } if answers.is_empty() && details == &valid
        ));
    }
}

#[test]
fn optional_producer_fields_do_not_hide_a_correlated_reply() {
    let mut reply = reply("call1", 0, "Question?", "Answer");
    reply["optional_metadata"] = json!({"future":true});
    let source = envelope(vec![reply]);
    let transcript = parse(
        TranscriptAgent::Codex,
        &lines(&[request("call1", &["Question?"]), message(&[&source])]),
        0,
    );
    assert!(
        matches!(&transcript.entries[1].kind, EntryKind::Notice { details: Some(details), .. } if details == &source)
    );
}

#[test]
fn answer_count_and_source_limits_fall_back_to_bounded_user_text() {
    let questions = vec!["Question?"; MAX_QUESTION_ANSWERS + 1];
    let source = envelope(
        (0..questions.len())
            .map(|index| reply("call1", index, "Question?", "Answer"))
            .collect(),
    );
    let transcript = parse(
        TranscriptAgent::Codex,
        &lines(&[request("call1", &questions), message(&[&source])]),
        0,
    );
    assert!(matches!(&transcript.entries[1].kind, EntryKind::User { text, .. } if text == &source));
    let source = envelope(vec![reply("call1", 0, "Question?", &"α".repeat(TEXT))]);
    let transcript = parse(
        TranscriptAgent::Codex,
        &lines(&[request("call1", &["Question?"]), message(&[&source])]),
        0,
    );
    assert!(
        matches!(&transcript.entries[1].kind, EntryKind::User { text, .. } if text.len() <= TEXT)
    );
    assert!(transcript.truncated);
}

#[test]
fn projected_question_and_answer_text_counts_toward_retention() {
    let source = envelope(vec![reply("call1", 0, "Question?", "Answer")]);
    let transcript = parse(
        TranscriptAgent::Codex,
        &lines(&[request("call1", &["Question?"]), message(&[&source])]),
        0,
    );
    assert_eq!(
        crate::session::entry_size(&transcript.entries[1]),
        "Question answered".len() + source.len() + "Question?".len() + "Answer".len()
    );
}

#[test]
fn consumed_question_ids_remain_bounded() {
    let mut projection = Projection {
        answered: (0..MAX_ENTRIES)
            .map(|index| {
                (
                    QuestionKey {
                        call: format!("old{index}"),
                        index: 0,
                    },
                    QuestionAnswer {
                        question: "Q".into(),
                        answer: "A".into(),
                    },
                )
            })
            .collect(),
        transcript: Transcript::default(),
        tools: HashMap::new(),
    };
    projection.codex(EntryId(1), &request("call1", &["Question?"]));
    let source = envelope(vec![reply("call1", 0, "Question?", "Answer")]);
    assert!(
        projection
            .question_reply(&message(&[&source]), &source)
            .is_none()
    );
    assert_eq!(projection.answered.len(), MAX_ENTRIES);
}

#[test]
fn remembered_question_text_has_a_separate_memory_bound() {
    let mut projection = Projection {
        answered: HashMap::from([(
            QuestionKey {
                call: "old".into(),
                index: 0,
            },
            QuestionAnswer {
                question: "Q".into(),
                answer: "A".repeat(MAX_TRANSCRIPT_TEXT - 4),
            },
        )]),
        transcript: Transcript::default(),
        tools: HashMap::new(),
    };
    projection.codex(EntryId(1), &request("call1", &["Question?"]));
    let source = envelope(vec![reply("call1", 0, "Question?", "Answer")]);
    assert!(
        projection
            .question_reply(&message(&[&source]), &source)
            .is_none()
    );
    assert_eq!(projection.answered.len(), 1);
}
