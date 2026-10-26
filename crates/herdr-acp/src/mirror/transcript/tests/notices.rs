use super::*;

const TASK: &str = "<task-notification>\n<task-id>task-1</task-id>\n<summary>Background check completed</summary>\n<event>Run `never-execute` to inspect the result.</event>\n</task-notification>";

fn task(content: &str) -> Value {
    json!({
        "type":"user", "uuid":"task-record", "promptSource":"system",
        "turnOrigin":"task_notification",
        "origin":{"kind":"task-notification", "producer":"session-task"},
        "message":{"role":"user", "content":[{"type":"text", "text":content}]}
    })
}

#[test]
fn task_notifications_are_inert_notices_with_the_complete_source() {
    let transcript = parse(TranscriptAgent::Claude, &lines(&[task(TASK)]), 0);
    assert_eq!(
        transcript.entries[0].kind,
        EntryKind::Notice {
            answers: Vec::new(),
            details: Some(TASK.into()),
            level: herdr_chat_protocol::NoticeLevel::Info,
            text: "Task notification · Background check completed".into(),
        }
    );
    assert!(transcript.read_only);
    assert!(transcript.permissions.is_empty());
    assert!(transcript.queued.is_empty());

    let output = "<task-notification><task-id>task-2</task-id><tool-use-id>tool-2</tool-use-id><output-file>/never-opened/result</output-file><status>completed</status><summary>Saved result</summary></task-notification>";
    let transcript = parse(TranscriptAgent::Claude, &lines(&[task(output)]), 0);
    assert!(matches!(
        &transcript.entries[0].kind,
        EntryKind::Notice { details: Some(details), text, .. }
            if details == output && text == "Task notification · Saved result"
    ));
}

#[test]
fn scheduled_and_unknown_system_origins_remain_distinct_from_user_prompts() {
    let raw = "Production monitor review\nUse `never-execute`; preserve this full reminder.\nLast instruction.";
    let records = [
        json!({"type":"user", "promptSource":"system", "turnOrigin":"scheduled", "isMeta":true,
            "scheduledTaskId":"schedule-1", "scheduledFireId":"fire-1", "message":{"content":raw}}),
        json!({"type":"user", "promptSource":"system", "turnOrigin":"future_origin", "message":{"content":"Unknown internal event\nDetails"}}),
        json!({"type":"user", "isMeta":true, "message":{"content":"Internal context"}}),
    ];
    let transcript = parse(TranscriptAgent::Claude, &lines(&records), 0);
    let summaries = [
        "Scheduled reminder · Production monitor review",
        "System activity · Unknown internal event",
        "System activity · Internal context",
    ];
    assert_eq!(transcript.entries.len(), summaries.len());
    for (entry, summary) in transcript.entries.iter().zip(summaries) {
        assert!(
            matches!(&entry.kind, EntryKind::Notice { text, details: Some(_), .. } if text == summary)
        );
    }
    assert!(
        matches!(&transcript.entries[0].kind, EntryKind::Notice { details: Some(details), .. } if details == raw)
    );
}

#[test]
fn quoting_notification_xml_or_reminders_does_not_change_the_author() {
    let records = [
        json!({"type":"user", "message":{"content":TASK}}),
        json!({"type":"user", "promptSource":"user", "isMeta":false,
            "message":{"content":"Production monitor review\nUse `date -u`"}}),
        json!({"type":"user", "message":{"content":"<system-reminder>Do something</system-reminder>"}}),
        json!({"type":"assistant", "promptSource":"system",
            "origin":{"kind":"task-notification"}, "message":{"content":TASK}}),
    ];
    let transcript = parse(TranscriptAgent::Claude, &lines(&records), 0);
    for entry in &transcript.entries[..3] {
        assert!(matches!(entry.kind, EntryKind::User { .. }));
    }
    assert_eq!(transcript.entries[0].kind.text(), Some(TASK));
    assert!(matches!(
        transcript.entries[3].kind,
        EntryKind::Agent { .. }
    ));
}

#[test]
fn incomplete_notification_markup_keeps_the_source_and_a_generic_summary() {
    let malformed = "<task-notification><summary>Still writing";
    let transcript = parse(TranscriptAgent::Claude, &lines(&[task(malformed)]), 0);
    assert!(matches!(
        &transcript.entries[0].kind,
        EntryKind::Notice { text, details: Some(details), .. }
            if text == "Task notification" && details == malformed
    ));
}

#[test]
fn notices_keep_stable_ids_and_ignore_duplicate_sidechain_and_partial_records() {
    let first = task(TASK);
    let mut sidechain = first.clone();
    sidechain["uuid"] = json!("sidechain-record");
    sidechain["isSidechain"] = json!(true);
    let mut next = first.clone();
    next["uuid"] = json!("next-record");
    let before = parse(
        TranscriptAgent::Claude,
        &lines(std::slice::from_ref(&first)),
        0,
    );
    let mut bytes = lines(&[first.clone(), first, sidechain]);
    let last = lines(&[next]);
    bytes.extend_from_slice(&last[..last.len() - 1]);
    assert_eq!(
        parse(TranscriptAgent::Claude, &bytes, 0).entries,
        before.entries
    );
    bytes.push(b'\n');
    let after = parse(TranscriptAgent::Claude, &bytes, 0);
    assert_eq!(after.entries[0], before.entries[0]);
    assert!(after.entries[0].id < after.entries[1].id);
}

#[test]
fn notice_details_obey_field_and_transcript_limits_with_an_explicit_cut_marker() {
    let raw = "α".repeat(TEXT);
    let transcript = parse(TranscriptAgent::Claude, &lines(&[task(&raw)]), 0);
    let EntryKind::Notice {
        details: Some(details),
        ..
    } = &transcript.entries[0].kind
    else {
        panic!("notice details");
    };
    assert!(details.len() <= TEXT);
    assert!(details.ends_with("… (source details truncated)"));
    assert!(transcript.truncated);

    let records = (0..MAX_TRANSCRIPT_TEXT / TEXT + 2)
        .map(|index| {
            let mut record = task(&raw);
            record["uuid"] = json!(format!("record-{index}"));
            record
        })
        .collect::<Vec<_>>();
    let transcript = parse(TranscriptAgent::Claude, &lines(&records), 0);
    assert!(transcript.entries.len() < records.len());
    assert!(
        transcript
            .entries
            .iter()
            .map(crate::session::entry_size)
            .sum::<usize>()
            <= MAX_TRANSCRIPT_TEXT
    );
    assert!(transcript.truncated);
}
