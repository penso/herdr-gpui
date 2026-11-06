use crate::overview::{
    Error,
    replies::{MAX_QUEUED, Phase, Presence, Replies, Reply},
};
use sheprd_client::protocol::AgentStatus;
use std::time::{Duration, Instant};

fn queued(panes: &[&str]) -> Replies {
    let mut replies = Replies::default();
    for pane in panes {
        assert!(replies.push(("local", pane), "boot", "agent", "go on"));
    }
    replies
}

fn here(status: AgentStatus) -> impl Fn(&Reply) -> Presence {
    move |_| Presence::Here(status)
}

fn refusal(code: &str) -> Error {
    Error::ReplyRefused {
        code: code.into(),
        message: "no".into(),
    }
}

#[test]
fn a_reply_waits_until_the_agent_is_at_its_prompt() {
    let now = Instant::now();
    let mut replies = queued(&["p"]);
    for status in [
        AgentStatus::Working,
        AgentStatus::Blocked,
        AgentStatus::Unknown,
    ] {
        assert!(replies.advance(now, here(status)).send.is_empty());
    }
    for status in [AgentStatus::Idle, AgentStatus::Done] {
        let mut replies = queued(&["p"]);
        let advance = replies.advance(now, here(status));
        assert_eq!(advance.send.len(), 1, "{status:?}");
        assert_eq!(advance.send[0].phase, Phase::Sending);
    }
}

#[test]
fn a_reply_going_out_is_sent_once() {
    let now = Instant::now();
    let mut replies = queued(&["p"]);
    assert_eq!(replies.advance(now, here(AgentStatus::Idle)).send.len(), 1);
    assert!(
        replies
            .advance(now, here(AgentStatus::Idle))
            .send
            .is_empty()
    );
}

#[test]
fn a_pane_takes_its_replies_in_order_one_at_a_time() {
    let now = Instant::now();
    let mut replies = Replies::default();
    assert!(replies.push(("local", "p"), "boot", "agent", "first"));
    assert!(replies.push(("local", "p"), "boot", "agent", "second"));
    let advance = replies.advance(now, here(AgentStatus::Idle));
    assert_eq!(advance.send.len(), 1);
    assert_eq!(advance.send[0].text, "first");
    assert!(
        replies
            .advance(now, here(AgentStatus::Idle))
            .send
            .is_empty()
    );
    replies.finish(advance.send[0].id, Ok(()), now);
    // The agent has not yet shown it took the first: the second holds.
    assert!(
        replies
            .advance(now, here(AgentStatus::Idle))
            .send
            .is_empty()
    );
    let later = now + Duration::from_secs(6);
    assert!(
        replies
            .advance(later, here(AgentStatus::Working))
            .send
            .is_empty()
    );
    let advance = replies.advance(later, here(AgentStatus::Idle));
    assert_eq!(advance.send[0].text, "second");
}

#[test]
fn replies_to_different_panes_do_not_wait_for_each_other() {
    let now = Instant::now();
    let mut replies = queued(&["a", "b"]);
    assert_eq!(replies.advance(now, here(AgentStatus::Idle)).send.len(), 2);
}

#[test]
fn the_same_pane_id_on_another_host_is_another_pane() {
    let now = Instant::now();
    let mut replies = Replies::default();
    assert!(replies.push(("one", "p"), "boot", "agent", "a"));
    assert!(replies.push(("two", "p"), "boot", "agent", "b"));
    assert_eq!(replies.advance(now, here(AgentStatus::Idle)).send.len(), 2);
}

#[test]
fn a_prompt_the_daemon_refused_as_blocked_waits_again() {
    let now = Instant::now();
    let mut replies = queued(&["p"]);
    let id = replies.advance(now, here(AgentStatus::Idle)).send[0].id;
    replies.finish(id, Err(refusal("agent_blocked")), now);
    let reply = replies.of("local", "p").next().unwrap();
    assert_eq!(reply.phase, Phase::Waiting);
    // Still blocked: nothing goes. Back at the prompt: it does, with no
    // settling, as nothing reached the pane.
    assert!(
        replies
            .advance(now, here(AgentStatus::Blocked))
            .send
            .is_empty()
    );
    assert_eq!(replies.advance(now, here(AgentStatus::Idle)).send.len(), 1);
}

#[test]
fn any_other_failure_stays_listed_and_is_not_retried() {
    let now = Instant::now();
    let mut replies = queued(&["p"]);
    let id = replies.advance(now, here(AgentStatus::Idle)).send[0].id;
    replies.finish(id, Err(refusal("agent_not_ready")), now);
    let reply = replies.of("local", "p").next().unwrap();
    assert_eq!(reply.phase, Phase::Failed("no".into()));
    assert!(
        replies
            .advance(now, here(AgentStatus::Idle))
            .send
            .is_empty()
    );
    assert!(replies.cancel(id));
    assert!(replies.is_empty());
}

#[test]
fn a_failed_reply_does_not_hold_up_the_next() {
    let now = Instant::now();
    let mut replies = queued(&["p", "p"]);
    let id = replies.advance(now, here(AgentStatus::Idle)).send[0].id;
    replies.finish(id, Err(refusal("agent_not_ready")), now);
    assert_eq!(replies.advance(now, here(AgentStatus::Idle)).send.len(), 1);
}

#[test]
fn a_reply_to_an_agent_that_left_is_dropped_never_sent() {
    let now = Instant::now();
    let mut replies = queued(&["p"]);
    let advance = replies.advance(now, |_| Presence::Gone);
    assert!(advance.send.is_empty());
    assert_eq!(advance.dropped.len(), 1);
    assert!(replies.is_empty());
}

#[test]
fn a_host_out_of_reach_keeps_its_replies_in_order() {
    let now = Instant::now();
    let mut replies = queued(&["p"]);
    let advance = replies.advance(now, |_| Presence::Unreachable);
    assert_eq!(advance, Default::default());
    assert!(!replies.is_empty());
}

#[test]
fn only_a_reply_not_yet_on_its_way_can_be_cancelled() {
    let now = Instant::now();
    let mut replies = queued(&["p", "q"]);
    assert!(replies.cancel(1));
    let id = replies.advance(now, here(AgentStatus::Idle)).send[0].id;
    assert!(!replies.cancel(id));
    assert!(!replies.is_empty());
}

#[test]
fn an_answer_for_a_cancelled_reply_is_ignored() {
    let mut replies = queued(&["p"]);
    assert!(replies.cancel(0));
    replies.finish(0, Ok(()), Instant::now());
    assert!(replies.is_empty());
}

#[test]
fn the_queue_is_bounded() {
    let mut replies = Replies::default();
    for _ in 0..MAX_QUEUED {
        assert!(replies.push(("local", "p"), "boot", "agent", "x"));
    }
    assert!(!replies.push(("local", "p"), "boot", "agent", "x"));
}
