use serde_json::json;

use super::*;

mod agents;

#[test]
fn known_tools_read_as_what_they_were_asked_to_do() {
    let cases = [
        (
            "Bash",
            json!({"command": "git status"}),
            ToolKind::Execute,
            "`git status`",
        ),
        (
            "functions.exec_command",
            json!(r#"{"cmd":"cargo test"}"#),
            ToolKind::Execute,
            "`cargo test`",
        ),
        (
            "Read",
            json!({"file_path": "src/a.rs"}),
            ToolKind::Read,
            "Read src/a.rs",
        ),
        (
            "Edit",
            json!({"file_path": "src/a.rs"}),
            ToolKind::Edit,
            "Edit src/a.rs",
        ),
        (
            "Grep",
            json!({"pattern": "TODO"}),
            ToolKind::Search,
            "Search for TODO",
        ),
        (
            "Task",
            json!({"description": "Review the patch"}),
            ToolKind::Subagent,
            "Review the patch",
        ),
    ];
    for (name, input, kind, title) in cases {
        let described = describe(name, &input);
        assert_eq!(described.kind, kind, "{name}");
        assert_eq!(described.title, title, "{name}");
    }
}

#[test]
fn a_named_file_is_a_location_unless_it_cannot_be_one() {
    let input = json!({"file_path": "src/a.rs"});
    assert_eq!(describe("Read", &input).locations, ["src/a.rs"]);
    let control = json!({"file_path": "src/a\u{1b}.rs"});
    assert!(describe("Read", &control).locations.is_empty());
    let long = json!({"file_path": "a".repeat(MAX_PATH + 1)});
    assert!(describe("Read", &long).locations.is_empty());
}

#[test]
fn unknown_tools_and_missing_arguments_keep_the_tools_name() {
    let described = describe("functions.request_user_input", &json!({"command": "ls"}));
    assert_eq!(described.kind, ToolKind::Other);
    assert_eq!(described.title, "functions.request_user_input");
    assert!(described.locations.is_empty());

    let described = describe("Bash", &json!({"timeout": 5}));
    assert_eq!(described.kind, ToolKind::Execute);
    assert_eq!(described.title, "Bash");
}

#[test]
fn a_title_stays_on_one_bounded_line() {
    let long = format!("{}\n{}", "a".repeat(MAX_TITLE), "b".repeat(MAX_TITLE));
    let described = describe("Task", &json!({"description": long}));
    assert!(described.title.len() <= MAX_TITLE);
    assert!(!described.title.chars().any(char::is_control));
}
