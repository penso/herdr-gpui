#![allow(clippy::unwrap_used, clippy::expect_used)]

use serde_json::json;

use super::*;

fn as_json(call: &wire::ToolCall) -> Value {
    serde_json::to_value(call).unwrap()
}

#[test]
fn calls_are_titled_by_what_they_do() {
    let cases = [
        ("Bash", json!({"command": "ls -la"}), "`ls -la`"),
        (
            "Bash",
            json!({"command": "ls -la\necho done"}),
            "`ls -la` (+1 more line)",
        ),
        ("Bash", json!({"command": "a\nb\nc"}), "`a` (+2 more lines)"),
        ("Read", json!({"file_path": "/a/b.rs"}), "Read /a/b.rs"),
        ("Grep", json!({"pattern": "fn main"}), "Search for fn main"),
        (
            "WebFetch",
            json!({"url": "https://x.test"}),
            "Fetch https://x.test",
        ),
        (
            "Task",
            json!({"description": "Explore the tests"}),
            "Explore the tests",
        ),
        ("mcp__github__issue", json!({}), "mcp__github__issue"),
        ("Bash", json!({}), "Bash"),
    ];
    for (name, input, title) in cases {
        assert_eq!(super::title(name, &input), title, "{name}");
    }
}

#[test]
fn an_edit_shows_its_diff_and_location() {
    let shown = as_json(&call(
        "t1",
        "Edit",
        &json!({"file_path": "/a/b.rs", "old_string": "x", "new_string": "y"}),
    ));
    assert_eq!(shown["kind"], "edit");
    assert_eq!(shown["content"][0]["type"], "diff");
    assert_eq!(shown["content"][0]["oldText"], "x");
    assert_eq!(shown["content"][0]["newText"], "y");
    assert_eq!(shown["locations"][0]["path"], "/a/b.rs");
    let several = as_json(&call(
        "t2",
        "MultiEdit",
        &json!({"file_path": "/a.rs", "edits": [
            {"old_string": "a", "new_string": "b"},
            {"old_string": "c", "new_string": "d"},
        ]}),
    ));
    assert_eq!(several["content"].as_array().unwrap().len(), 2);
    let written = as_json(&call(
        "t3",
        "Write",
        &json!({"file_path": "/n.txt", "content": "hi"}),
    ));
    assert!(
        written["content"][0]
            .get("oldText")
            .is_none_or(Value::is_null)
    );
    let read = as_json(&call(
        "t4",
        "Read",
        &json!({"file_path": "/a.rs", "offset": 40}),
    ));
    assert_eq!(read["locations"][0]["line"], 40);
}

#[test]
fn a_command_of_several_lines_is_shown_whole() {
    let shown = as_json(&call(
        "t1",
        "Bash",
        &json!({"command": "echo hi\nrm -rf build"}),
    ));
    assert_eq!(
        shown["content"][0]["content"]["text"],
        "echo hi\nrm -rf build"
    );
    let short = as_json(&call("t2", "Bash", &json!({"command": "ls"})));
    assert!(
        short
            .get("content")
            .is_none_or(|content| content.as_array().is_some_and(Vec::is_empty))
    );
}

#[test]
fn a_plan_to_approve_is_shown_with_its_call() {
    let shown = as_json(&call("t1", EXIT_PLAN, &json!({"plan": "# Plan\n1. Fix"})));
    assert_eq!(shown["kind"], "switch_mode");
    assert_eq!(shown["content"][0]["content"]["text"], "# Plan\n1. Fix");
}

#[test]
fn results_show_output_but_not_what_the_call_already_shows() {
    assert!(result("Edit", &json!("File updated"), false).is_none());
    assert!(result("Read", &json!("1\tfn main"), false).is_none());
    let failed = result("Edit", &json!("String not found"), true).unwrap();
    assert_eq!(failed.len(), 1);
    let output = result(
        "mcp__shot",
        &json!([
            {"type": "text", "text": "Took it"},
            {"type": "image", "source": {"type": "base64", "media_type": "image/png", "data": "iVBOR"}},
            {"type": "tool_reference", "tool_name": "x"},
        ]),
        false,
    )
    .unwrap();
    assert_eq!(output.len(), 2);
    assert!(matches!(
        &output[1],
        wire::ToolCallContent::Content(content)
            if matches!(&content.content, wire::ContentBlock::Image(image) if image.mime_type == "image/png")
    ));
    assert!(result("Bash", &json!(""), false).unwrap().is_empty());
}

#[test]
fn only_a_single_question_is_asked_as_choices() {
    let one = json!({"questions": [{"question": "Which?"}]});
    assert_eq!(question(&one), Some("Which?"));
    let two = json!({"questions": [{"question": "A?"}, {"question": "B?"}]});
    assert_eq!(question(&two), None);
}
