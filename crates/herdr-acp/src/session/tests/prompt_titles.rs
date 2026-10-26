use super::*;

#[test]
fn a_sent_prompt_is_passed_on_for_herdr() {
    let mut harness = ready();
    prompt(&mut harness, 0, "fix the bug");
    assert_eq!(*harness.prompts.borrow(), ["fix the bug"]);
}

#[test]
fn a_queued_prompt_is_passed_on_when_it_is_sent() {
    let mut harness = ready();
    let id = prompt(&mut harness, 0, "first");
    harness.command(
        0,
        Command::Prompt {
            text: "second".to_owned(),
            attachments: Vec::new(),
        },
    );
    assert_eq!(*harness.prompts.borrow(), ["first"]);
    harness.respond(id, json!({"stopReason": "end_turn"}));
    assert_eq!(*harness.prompts.borrow(), ["first", "second"]);
}

#[test]
fn a_refused_prompt_is_not_passed_on() {
    let mut harness = ready();
    harness.command(
        0,
        Command::Prompt {
            text: "   ".to_owned(),
            attachments: Vec::new(),
        },
    );
    assert!(harness.prompts.borrow().is_empty());
}

#[test]
fn a_prompt_the_agent_never_got_is_not_passed_on() {
    let mut harness = ready();
    let (_, gone) = bounded(1);
    drop(std::mem::replace(&mut harness.agent, gone));
    harness.command(
        0,
        Command::Prompt {
            text: "lost".to_owned(),
            attachments: Vec::new(),
        },
    );
    assert!(harness.prompts.borrow().is_empty());
}
