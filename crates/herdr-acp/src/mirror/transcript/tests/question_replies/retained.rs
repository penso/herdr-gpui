use super::*;

fn retained(call: &str, question: &str, answer: &str) -> Value {
    json!({"type":"retained_context", "payload":{
        "type":"verified_answer", "call_id":call, "turn_id":"turn1", "acceptance_order":1,
        "questions":[{"question":question,"answer":answer}]
    }})
}

fn compacted(answer: &Value) -> Value {
    let mut answer = answer["payload"].clone();
    answer["order"] = json!(1);
    answer.as_object_mut().unwrap().remove("type");
    answer.as_object_mut().unwrap().remove("acceptance_order");
    json!({"type":"compacted", "payload":{
        "message":"Context summary", "retained_context":{"verified_answers":[answer]},
        "replacement_history":[{"role":"assistant", "content":"Must never be replayed"}]
    }})
}

#[test]
fn native_retained_answers_work_without_the_original_request() {
    let record = retained("oldcall", "Which approach?", "Free text");
    for record in [record.clone(), compacted(&record)] {
        let transcript = parse(TranscriptAgent::Codex, &lines(&[record]), 100);
        assert!(transcript.entries.iter().any(|entry| matches!(&entry.kind,
            EntryKind::Notice { answers, details: Some(source), .. }
            if answers == &vec![QuestionAnswer { question:"Which approach?".into(), answer:"Free text".into() }] && source.contains("oldcall")
        )));
        assert!(
            !transcript
                .entries
                .iter()
                .any(|entry| matches!(entry.kind, EntryKind::Agent { .. }))
        );
        assert!(transcript.permissions.is_empty());
        assert!(transcript.queued.is_empty());
    }
}

#[test]
fn partial_retained_overlap_keeps_new_rows_in_native_order() {
    let first = retained("call1", "First?", "One");
    let mut more = first.clone();
    more["payload"]["questions"] = json!([
        {"question":"First?", "answer":"One"},
        {"question":"Second?", "answer":""},
        {"question":"Third?", "answer":"Three"},
    ]);
    let transcript = parse(TranscriptAgent::Codex, &lines(&[first, more]), 0);
    assert!(matches!(&transcript.entries[1].kind,
        EntryKind::Notice { answers, .. } if answers.iter().map(|a| a.question.as_str()).eq(["Second?", "Third?"]) && answers[0].answer.is_empty()
    ));
}

#[test]
fn interleaved_native_retained_raw_and_compacted_answers_do_not_repeat_cards_or_xml() {
    let record = retained("call1", "Question?", "Answer");
    let source = envelope(vec![reply("call1", 0, "Question?", "Answer")]);
    for order in [
        vec![record.clone(), message(&[&source]), compacted(&record)],
        vec![message(&[&source]), record.clone(), compacted(&record)],
    ] {
        let mut records = vec![request("call1", &["Question?"])];
        records.extend(order);
        let transcript = parse(TranscriptAgent::Codex, &lines(&records), 0);
        assert_eq!(transcript.entries.iter().filter(|entry| matches!(&entry.kind, EntryKind::Notice { answers, .. } if !answers.is_empty())).count(), 1);
        assert!(
            !transcript
                .entries
                .iter()
                .any(|entry| matches!(entry.kind, EntryKind::User { .. }))
        );
        assert!(transcript.entries.iter().any(|entry| matches!(&entry.kind,
            EntryKind::Notice { details: Some(details), .. } if details == &source
        )));
    }
}

#[test]
fn retained_answers_correlate_later_raw_replies_without_the_old_tool() {
    let same = envelope(vec![reply("call1", 0, "Question?", "Answer")]);
    let changed = envelope(vec![reply("call1", 0, "Question?", "Updated")]);
    let unmatched = envelope(vec![reply("call1", 0, "Different question?", "Other")]);
    let transcript = parse(
        TranscriptAgent::Codex,
        &lines(&[
            retained("call1", "Question?", "Answer"),
            message(&[&same]),
            message(&[&changed]),
            message(&[&unmatched]),
        ]),
        100,
    );
    assert!(matches!(&transcript.entries[1].kind,
        EntryKind::Notice { answers, details: Some(details), .. } if answers.is_empty() && details == &same
    ));
    assert!(matches!(&transcript.entries[2].kind,
        EntryKind::Notice { answers, .. } if answers[0].answer == "Updated"
    ));
    assert!(
        matches!(&transcript.entries[3].kind, EntryKind::User { text, .. } if text == &unmatched)
    );
}

#[test]
fn empty_native_compaction_has_no_blank_source_expander() {
    let transcript = parse(
        TranscriptAgent::Codex,
        &lines(&[json!({"type":"compacted", "payload":{"message":""}})]),
        0,
    );
    assert!(matches!(&transcript.entries[0].kind,
        EntryKind::Notice { text, details: None, .. } if text == "Context compacted"
    ));
}

#[test]
fn changed_answers_remain_visible_and_explicit_user_pastes_stay_literal() {
    let source = envelope(vec![reply("call1", 0, "Question?", "Changed answer")]);
    let mut pasted = message(&[&source]);
    pasted["metadata"]["client_authored"] = json!(true);
    let transcript = parse(
        TranscriptAgent::Codex,
        &lines(&[
            request("call1", &["Question?"]),
            retained("call1", "Question?", "Original answer"),
            message(&[&source]),
            retained("call1", "Question?", "Updated retained answer"),
            pasted,
        ]),
        0,
    );
    let answers = transcript
        .entries
        .iter()
        .filter_map(|entry| match &entry.kind {
            EntryKind::Notice { answers, .. } => {
                answers.first().map(|answer| answer.answer.as_str())
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        answers,
        [
            "Original answer",
            "Changed answer",
            "Updated retained answer"
        ]
    );
    assert!(
        matches!(&transcript.entries.last().unwrap().kind, EntryKind::User { text, .. } if text == &source)
    );
}

#[test]
fn retained_rows_are_typed_bounded_and_not_inferred_from_pasted_json() {
    let record = retained("call1", "Question?", "Answer");
    let mut invalid = Vec::new();
    for (field, value) in [
        ("call_id", json!("")),
        ("turn_id", json!(false)),
        ("acceptance_order", json!(-1)),
        (
            "questions",
            json!([{"question":"Question?", "answer":false}]),
        ),
    ] {
        let mut record = record.clone();
        record["payload"][field] = value;
        invalid.push(record);
    }
    let mut oversized = record.clone();
    oversized["payload"]["questions"] = json!(vec![
        json!({"question":"Question?","answer":"Answer"});
        MAX_QUESTION_ANSWERS + 1
    ]);
    invalid.push(oversized);
    let mut oversized = record.clone();
    oversized["payload"]["questions"][0]["answer"] = json!("α".repeat(TEXT));
    invalid.push(oversized);
    for record in invalid {
        let transcript = parse(TranscriptAgent::Codex, &lines(&[record]), 0);
        assert!(matches!(&transcript.entries[0].kind,
            EntryKind::Notice { answers, text, details: Some(source), .. }
            if answers.is_empty() && text == "Retained question context" && source.len() <= TEXT
        ));
    }
    let source = record.to_string();
    let transcript = parse(TranscriptAgent::Codex, &lines(&[message(&[&source])]), 0);
    assert!(matches!(&transcript.entries[0].kind, EntryKind::User { text, .. } if text == &source));
}

#[test]
fn compaction_only_exposes_its_bounded_message_and_preserves_later_entries() {
    let record = json!({"type":"compacted", "payload":{
        "message":"α".repeat(TEXT),
        "replacement_history":[{"type":"message", "role":"user", "content":[{"type":"input_text","text":"Hidden history"}]}]
    }});
    let transcript = parse(
        TranscriptAgent::Codex,
        &lines(&[record, message(&["Later message"])]),
        0,
    );
    assert!(matches!(&transcript.entries[0].kind,
        EntryKind::Notice { text, details: Some(details), .. }
        if text == "Context compacted" && details.len() <= TEXT && !details.contains("Hidden history")
    ));
    assert!(
        matches!(&transcript.entries[1].kind, EntryKind::User { text, .. } if text == "Later message")
    );
    assert!(transcript.truncated);
}
