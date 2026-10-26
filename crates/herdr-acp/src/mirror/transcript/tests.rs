#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;
use serde_json::json;
use std::io::Write as _;

mod bounds;
mod codex_messages;
mod metadata;
mod notices;
mod question_replies;

fn lines(values: &[Value]) -> Vec<u8> {
    values
        .iter()
        .map(|value| format!("{value}\n"))
        .collect::<String>()
        .into_bytes()
}

#[test]
fn codex_uses_canonical_messages_and_never_replays_permissions() {
    let bytes = lines(&[
        json!({"type":"session_meta","payload":{"id":"thread1","cwd":"/work"}}),
        json!({"type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"Explain $x^2$"}]}}),
        json!({"type":"event_msg","payload":{"type":"user_message","message":"Explain $x^2$"}}),
        json!({"type":"response_item","payload":{"type":"function_call","call_id":"c1","name":"read_file","arguments":"{\"path\":\"notes.txt\"}"}}),
        json!({"type":"response_item","payload":{"type":"function_call_output","call_id":"c1","output":"saved output"}}),
        json!({"type":"event_msg","payload":{"type":"exec_approval_request","call_id":"c1","command":["do-not-run"]}}),
        json!({"type":"response_item","payload":{"type":"message","role":"assistant","content":[{"type":"output_text","text":"\\[x^2\\]"}]}}),
    ]);
    let transcript = parse(TranscriptAgent::Codex, &bytes, 0);
    assert!(transcript.read_only);
    assert_eq!(transcript.entries.len(), 3);
    assert_eq!(transcript.session.unwrap().id, "thread1");
    assert!(transcript.permissions.is_empty());
    let EntryKind::Tool(tool) = &transcript.entries[1].kind else {
        panic!("tool")
    };
    assert_eq!(tool.status, ToolStatus::Completed);
    assert_eq!(
        tool.content.last(),
        Some(&ToolContent::Text {
            text: "saved output".into()
        })
    );
    assert_eq!(transcript.entries[2].kind.text(), Some("\\[x^2\\]"));
}

#[test]
fn claude_combines_tool_results_and_ignores_duplicate_and_sidechain_records() {
    let assistant = json!({"type":"assistant","uuid":"a1","sessionId":"s1","message":{"content":[
        {"type":"text","text":"Checking"},
        {"type":"tool_use","id":"t1","name":"Read","input":{"file_path":"/never-opened"}}
    ]}});
    let bytes = lines(&[
        json!({"type":"user","uuid":"u1","message":{"content":"Hello"}}),
        assistant.clone(),
        assistant,
        json!({"type":"assistant","uuid":"side","isSidechain":true,"message":{"content":[{"type":"text","text":"Other agent"}]}}),
        json!({"type":"user","uuid":"r1","message":{"content":[{"type":"tool_result","tool_use_id":"t1","is_error":true,"content":[{"type":"text","text":"No file"}]}]}}),
    ]);
    let transcript = parse(TranscriptAgent::Claude, &bytes, 0);
    assert_eq!(transcript.entries.len(), 3);
    assert!(
        transcript
            .entries
            .windows(2)
            .all(|pair| pair[0].id < pair[1].id)
    );
    let EntryKind::Tool(tool) = &transcript.entries[2].kind else {
        panic!("tool")
    };
    assert_eq!(tool.status, ToolStatus::Failed);
    assert!(transcript.permissions.is_empty());
}

#[test]
fn partial_lines_wait_and_stable_entries_keep_their_ids() {
    let first = lines(&[json!({"type":"user","message":{"content":"one"}})]);
    let second = lines(&[json!({"type":"assistant","message":{"content":"two"}})]);
    let mut partial = first.clone();
    partial.extend_from_slice(&second[..second.len() - 1]);
    let before = parse(TranscriptAgent::Claude, &partial, 50);
    assert_eq!(before.entries.len(), 1);
    partial.push(b'\n');
    let after = parse(TranscriptAgent::Claude, &partial, 50);
    assert_eq!(after.entries.len(), 2);
    assert_eq!(before.entries[0], after.entries[0]);
    assert!(after.truncated);
}

#[test]
fn file_changes_and_truncation_are_observed_without_writes_to_the_source() {
    let mut file = tempfile::NamedTempFile::new().unwrap();
    let mut source = Source::new(file.path().to_owned(), TranscriptAgent::Claude);
    let first = lines(&[json!({"type":"user","message":{"content":"one"}})]);
    file.write_all(&first).unwrap();
    assert_eq!(source.read().unwrap().unwrap().entries.len(), 1);
    assert!(source.read().unwrap().is_none());
    let second = lines(&[json!({"type":"assistant","message":{"content":"two"}})]);
    file.write_all(&second).unwrap();
    assert_eq!(source.read().unwrap().unwrap().entries.len(), 2);
    file.as_file().set_len(0).unwrap();
    assert!(source.read().unwrap().unwrap().entries.is_empty());
    assert_eq!(file.as_file().metadata().unwrap().len(), 0);
}

#[test]
fn unicode_and_transcript_limits_keep_the_latest_complete_entries() {
    assert_eq!(limited("αβγ", 3), "α");
    let bytes = lines(
        &(0..MAX_ENTRIES + 10)
            .map(|i| json!({"type":"user","message":{"content":format!("message {i}")}}))
            .collect::<Vec<_>>(),
    );
    let transcript = parse(TranscriptAgent::Claude, &bytes, 0);
    assert_eq!(transcript.entries.len(), MAX_ENTRIES);
    assert!(transcript.truncated);
    assert_eq!(transcript.entries[0].kind.text(), Some("message 10"));
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
fn oversized_and_malformed_records_do_not_hide_later_valid_messages() {
    let mut bytes = vec![b' '; RECORD + 1];
    bytes.extend_from_slice(b"\ninvalid json\n");
    bytes.extend(lines(&[
        json!({"type":"user","message":{"content":"visible"}}),
    ]));
    let transcript = parse(TranscriptAgent::Claude, &bytes, 0);
    assert_eq!(transcript.entries.len(), 1);
    assert_eq!(transcript.entries[0].kind.text(), Some("visible"));
    assert!(transcript.truncated);
}

#[test]
fn a_large_files_tail_keeps_complete_records_and_absolute_entry_ids() {
    let mut file = tempfile::NamedTempFile::new().unwrap();
    // The window begins in an oversized prefix record, which is skipped.
    file.write_all(&vec![b' '; TAIL as usize + 20]).unwrap();
    file.write_all(b"\n").unwrap();
    let first = lines(&[json!({"type":"user","message":{"content":"retained"}})]);
    file.write_all(&first).unwrap();
    let mut source = Source::new(file.path().to_owned(), TranscriptAgent::Claude);
    let before = source.read().unwrap().unwrap();
    assert_eq!(before.entries.len(), 1);
    assert!(before.truncated);
    file.write_all(&lines(&[
        json!({"type":"assistant","message":{"content":"new"}}),
    ]))
    .unwrap();
    let after = source.read().unwrap().unwrap();
    assert_eq!(after.entries.len(), 2);
    assert_eq!(before.entries[0], after.entries[0]);
}

#[test]
fn directories_are_rejected_before_reading() {
    let directory = tempfile::tempdir().unwrap();
    let mut source = Source::new(directory.path().to_owned(), TranscriptAgent::Claude);
    assert!(matches!(source.read().unwrap_err(), Error::NotRegularFile));
}

#[test]
fn a_path_missing_for_one_read_is_read_again_once_replaced() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("log.jsonl");
    let mut source = Source::new(path.clone(), TranscriptAgent::Claude);
    assert!(matches!(source.read().unwrap_err(), Error::Io(_)));
    std::fs::write(
        &path,
        lines(&[json!({"type":"user","message":{"role":"user","content":"hi"}})]),
    )
    .unwrap();
    assert_eq!(source.read().unwrap().unwrap().entries.len(), 1);
}
