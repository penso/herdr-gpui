//! Going back to before an earlier prompt, as a fork of pi's session.

use super::*;

const FORK: &str = "5f1c9a2e-0000-4000-8000-00000000000b";

fn asked(text: &str, occurrence: usize) -> Inbound {
    request(
        9,
        out::REWIND,
        json!({"occurrence": occurrence, "sessionId": SESSION, "text": text}),
    )
}

fn messages() -> Value {
    json!({"messages": [
        {"entryId": "e1", "text": "again"},
        {"entryId": "e2", "text": "two"},
        {"entryId": "e3", "text": "again"},
    ]})
}

#[test]
fn it_forks_from_the_prompts_message_and_goes_on_in_the_new_session() {
    let mut bridge = opened();
    let outs = bridge.on_session(asked("again", 2));
    let list = harness(&outs[0]);
    assert_eq!(list["type"], "get_fork_messages");
    let outs = bridge.on_harness(&done(list, messages()));
    let fork = harness(&outs[0]);
    assert_eq!(fork["type"], "fork");
    assert_eq!(fork["entryId"], "e1");
    let outs = bridge.on_harness(&done(fork, json!({"text": "again", "cancelled": false})));
    let state = harness(&outs[0]);
    assert_eq!(state["type"], "get_state");
    let outs = bridge.on_harness(&done(state, json!({"sessionId": FORK})));
    assert_eq!(response(&outs[0], 9).as_ref().unwrap()["sessionId"], FORK);
    assert_eq!(bridge.session, FORK);
}

#[test]
fn an_extension_that_refuses_keeps_the_session() {
    let mut bridge = opened();
    let outs = bridge.on_session(asked("two", 1));
    let outs = bridge.on_harness(&done(harness(&outs[0]), messages()));
    let outs = bridge.on_harness(&done(
        harness(&outs[0]),
        json!({"text": "two", "cancelled": true}),
    ));
    assert!(response(&outs[0], 9).is_err());
    assert_eq!(bridge.session, SESSION);
}

#[test]
fn a_prompt_pi_does_not_have_fails_without_forking() {
    let mut bridge = opened();
    let outs = bridge.on_session(asked("gone", 1));
    let outs = bridge.on_harness(&done(harness(&outs[0]), messages()));
    assert!(response(&outs[0], 9).is_err());
}
