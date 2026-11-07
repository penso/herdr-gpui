//! Going back to before an earlier prompt, as a fork of the thread.

use super::*;

const FORK: &str = "01a160d5-0000-7262-b413-0ce5180918ce";

fn asked(text: &str, occurrence: usize) -> Inbound {
    request(
        9,
        out::REWIND,
        json!({"occurrence": occurrence, "sessionId": THREAD, "text": text}),
    )
}

fn turn(id: &str, text: &str) -> Value {
    json!({"id": id, "items": [{"type": "userMessage", "content": [{"type": "text", "text": text}]}]})
}

#[test]
fn it_forks_through_the_turn_before_the_prompt_and_goes_on_there() {
    let mut bridge = opened();
    let outs = bridge.on_session(asked("two", 1));
    let list = harness(&outs[0]);
    assert_eq!(list["method"], "thread/turns/list");
    assert_eq!(list["params"]["threadId"], THREAD);
    let turns = json!({"data": [turn("t3", "three"), turn("t2", "two"), turn("t1", "one")]});
    let outs = bridge.on_harness(&answer(&list["id"], turns));
    let fork = harness(&outs[0]);
    assert_eq!(fork["method"], "thread/fork");
    assert_eq!(fork["params"]["threadId"], THREAD);
    assert_eq!(fork["params"]["lastTurnId"], "t1");
    assert_eq!(fork["params"]["cwd"], "/work");

    let outs = bridge.on_harness(&answer(&fork["id"], json!({"thread": {"id": FORK}})));
    assert_eq!(
        response(&outs[0], 9).as_ref().unwrap()["sessionId"],
        FORK,
        "the session goes on in the fork"
    );
    assert_eq!(bridge.thread.as_deref(), Some(FORK));
}

#[test]
fn the_first_prompt_leaves_nothing_to_fork_so_a_thread_starts() {
    let mut bridge = opened();
    let outs = bridge.on_session(asked("one", 1));
    let list = harness(&outs[0]);
    let turns = json!({"data": [turn("t2", "two"), turn("t1", "one")]});
    let outs = bridge.on_harness(&answer(&list["id"], turns));
    assert_eq!(harness(&outs[0])["method"], "thread/start");
}

#[test]
fn a_prompt_codex_does_not_have_fails_without_forking() {
    let mut bridge = opened();
    let outs = bridge.on_session(asked("gone", 1));
    let list = harness(&outs[0]);
    let outs = bridge.on_harness(&answer(&list["id"], json!({"data": [turn("t1", "one")]})));
    assert!(response(&outs[0], 9).is_err());
    assert_eq!(bridge.thread.as_deref(), Some(THREAD));
}

#[test]
fn a_refused_fork_stays_on_the_thread() {
    let mut bridge = opened();
    let outs = bridge.on_session(asked("two", 1));
    let list = harness(&outs[0]);
    let turns = json!({"data": [turn("t2", "two"), turn("t1", "one")]});
    let outs = bridge.on_harness(&answer(&list["id"], turns));
    let fork = harness(&outs[0]);
    let error = json!({"id": fork["id"], "error": {"message": "no such turn"}});
    let outs = bridge.on_harness(&error);
    assert!(response(&outs[0], 9).is_err());
    assert_eq!(bridge.thread.as_deref(), Some(THREAD));
}
