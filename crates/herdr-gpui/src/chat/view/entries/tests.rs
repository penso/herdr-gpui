use super::*;
use herdr_chat_protocol::ToolContent;

#[core::prelude::v1::test]
fn question_tool_labels_do_not_change_unknown_tools() {
    assert_eq!(tool_header("request_user_input"), "Ask user");
    assert_eq!(tool_header("request_user_input_async"), "Ask user");
    assert_eq!(
        tool_header("some_request_user_input"),
        "some_request_user_input"
    );
}

#[core::prelude::v1::test]
fn expanded_tool_content_keeps_long_lines_and_every_diff_line() {
    let text = format!("{}\n{}\nlast output", "x".repeat(900), "line\n".repeat(20));
    assert_eq!(
        tool_text(&ToolContent::Text { text: text.clone() }),
        Some(text.clone())
    );
    let diff = tool_text(&ToolContent::Diff {
        path: "example.txt".into(),
        old: Some(text.clone()),
        new: "replacement\nfinal line".into(),
    });
    assert_eq!(
        diff,
        Some(format!(
            "example.txt\nBefore:\n{text}\nAfter:\nreplacement\nfinal line"
        ))
    );
}
