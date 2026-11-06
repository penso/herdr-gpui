use super::*;

use crate::session::entry_size;

#[test]
fn a_mirrored_transcript_keeps_no_more_than_a_live_chat() {
    let input = "x".repeat(TEXT);
    let records = (0..MAX_TRANSCRIPT_TEXT / TEXT + 4)
        .map(|index| {
            json!({"type":"assistant","uuid":format!("a{index}"),"message":{"content":[
                {"type":"tool_use","id":format!("t{index}"),"name":"Read","input":input}
            ]}})
        })
        .collect::<Vec<_>>();
    let transcript = parse(TranscriptAgent::Claude, &lines(&records), 0);
    assert!(transcript.truncated);
    assert!(transcript.entries.len() < records.len());
    assert_eq!(keep_from(&transcript.entries), 0);
    let kept: usize = transcript.entries.iter().map(entry_size).sum();
    assert!(kept <= MAX_TRANSCRIPT_TEXT, "{kept}");
}

#[test]
fn every_cut_shown_text_marks_the_transcript_truncated() {
    let long = "x".repeat(TEXT + 1);
    for record in [
        json!({"type":"assistant","message":{"role":"assistant","content":[{"type":"thinking","thinking":long}]}}),
        json!({"type":"assistant","message":{"role":"assistant","content":[{"type":"tool_use","id":"t1","name":"Read","input":long}]}}),
        json!({"type":"response_item","payload":{"type":"reasoning","summary":[{"type":"summary_text","text":long}]}}),
    ] {
        let agent = if record["type"] == "response_item" {
            TranscriptAgent::Codex
        } else {
            TranscriptAgent::Claude
        };
        let transcript = parse(agent, &lines(std::slice::from_ref(&record)), 0);
        assert!(!transcript.entries.is_empty(), "{record}");
        assert!(transcript.truncated, "{record}");
    }
}
