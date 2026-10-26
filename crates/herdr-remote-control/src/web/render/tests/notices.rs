use super::*;
use herdr_chat_protocol::{NoticeLevel, QuestionAnswer};

#[test]
fn rich_notice_metadata_keeps_the_remote_summary_inert() {
    let mut transcript = Transcript::new("Claude");
    transcript.entries.push(entry(
        1,
        EntryKind::Notice {
            answers: vec![QuestionAnswer {
                question: "Historical question".into(),
                answer: "Private answer".into(),
            }],
            details: Some("Private original record".into()),
            level: NoticeLevel::Warning,
            text: "Summary <script>".into(),
        },
    ));
    let html = chat(
        &workspace(vec![]),
        &row("p1"),
        &View {
            transcript,
            connected: true,
        },
        0,
    )
    .into_string();
    assert!(html.contains("Summary &lt;script&gt;"));
    assert!(!html.contains("Historical question"));
    assert!(!html.contains("Private answer"));
    assert!(!html.contains("Private original record"));
    assert!(!html.contains("&quot;permission&quot;"));
}
