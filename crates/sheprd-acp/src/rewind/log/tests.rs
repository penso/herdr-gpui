use serde_json::json;

use super::*;

fn rewind(text: &str, occurrence: usize) -> Rewind {
    Rewind {
        occurrence,
        session_id: "s".to_owned(),
        text: text.to_owned(),
    }
}

fn log(records: &[Value]) -> Vec<u8> {
    let mut bytes = Vec::new();
    for record in records {
        bytes.extend(serde_json::to_vec(record).unwrap());
        bytes.push(b'\n');
    }
    bytes
}

fn user(text: &str) -> Value {
    json!({"type": "user", "message": {"content": [{"type": "text", "text": text}]}})
}

fn assistant(uuid: &str) -> Value {
    json!({"type": "assistant", "uuid": uuid, "message": {"content": []}})
}

#[test]
fn the_anchor_is_the_last_assistant_message_before_the_prompt() {
    let bytes = log(&[
        user("one"),
        assistant("a1"),
        assistant("a2"),
        user("two"),
        assistant("a3"),
    ]);
    assert_eq!(
        anchor(bytes.as_slice(), &rewind("two", 1))
            .unwrap()
            .as_deref(),
        Some("a2")
    );
}

#[test]
fn the_first_prompt_has_nothing_to_fork() {
    let bytes = log(&[user("one"), assistant("a1")]);
    assert_eq!(anchor(bytes.as_slice(), &rewind("one", 1)).unwrap(), None);
}

#[test]
fn a_repeated_prompt_is_counted_from_the_newest() {
    let bytes = log(&[
        user("again"),
        assistant("a1"),
        user("again"),
        assistant("a2"),
        user("again"),
        assistant("a3"),
    ]);
    let found = |occurrence| {
        anchor(bytes.as_slice(), &rewind("again", occurrence))
            .unwrap()
            .map(|uuid| uuid.to_owned())
    };
    assert_eq!(found(1).as_deref(), Some("a2"));
    assert_eq!(found(2).as_deref(), Some("a1"));
    assert_eq!(found(3), None);
}

#[test]
fn tool_results_sub_agents_and_harness_messages_are_not_prompts() {
    let bytes = log(&[
        json!({"type": "user", "isMeta": true, "message": {"content": "two"}}),
        json!({"type": "user", "isSidechain": true, "message": {"content": "two"}}),
        json!({"type": "user", "message": {"content": [{"type": "tool_result", "content": "two"}]}}),
        json!({"type": "assistant", "isSidechain": true, "uuid": "side"}),
        assistant("a1"),
        json!({"type": "user", "message": {"content": "two"}}),
    ]);
    assert_eq!(
        anchor(bytes.as_slice(), &rewind("two", 1))
            .unwrap()
            .as_deref(),
        Some("a1")
    );
    assert!(matches!(
        anchor(bytes.as_slice(), &rewind("two", 2)),
        Err(Error::NoPrompt)
    ));
}

#[test]
fn a_prompt_the_log_lacks_is_an_error_and_never_a_guess() {
    let bytes = log(&[user("one"), assistant("a1")]);
    assert!(matches!(
        anchor(bytes.as_slice(), &rewind("edited elsewhere", 1)),
        Err(Error::NoPrompt)
    ));
}

#[test]
fn lines_that_are_not_records_are_skipped() {
    let mut bytes = b"not json\n".to_vec();
    bytes.extend(log(&[assistant("a1"), user("two")]));
    assert_eq!(
        anchor(bytes.as_slice(), &rewind("two", 1))
            .unwrap()
            .as_deref(),
        Some("a1")
    );
}
