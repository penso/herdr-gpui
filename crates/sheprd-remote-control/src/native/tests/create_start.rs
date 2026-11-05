//! What a new chat starts with: the mode goes first, a session still
//! starting is asked again, and a refusal is reported so the first prompt
//! waits.

use crate::native::create::{Start, apply_start};
use serde_json::json;
use sheprd_chat_protocol::{Command, OptionValue, Rejection};
use std::{
    cell::RefCell, collections::VecDeque, convert::Infallible, future::ready, time::Duration,
};

const WAIT: Duration = Duration::from_secs(5);

/// Runs `apply_start` against answers given in order, then `Ok(None)`, and
/// returns whether it took and what was sent.
async fn run(
    start: &Start,
    wait: Duration,
    answers: Vec<Option<Rejection>>,
) -> (bool, Vec<Command>) {
    let answers = RefCell::new(VecDeque::from(answers));
    let sent = RefCell::new(Vec::new());
    let took = apply_start(start, wait, |command| {
        sent.borrow_mut().push(command);
        ready(Ok::<_, Infallible>(
            answers.borrow_mut().pop_front().flatten(),
        ))
    })
    .await;
    (took, sent.into_inner())
}

fn plan() -> Start {
    Start {
        mode: Some("plan".into()),
        ..Start::default()
    }
}

#[tokio::test]
async fn the_mode_goes_before_auto_accepting() {
    let start = Start {
        auto_accept: true,
        ..plan()
    };
    let (took, sent) = run(&start, WAIT, Vec::new()).await;
    assert!(took);
    assert_eq!(
        sent,
        vec![
            Command::SetMode { id: "plan".into() },
            Command::SetAutoAccept { on: true }
        ]
    );
}

#[tokio::test]
async fn a_mode_setting_is_set_through_its_option() {
    let start = Start {
        option: Some("mode".into()),
        ..plan()
    };
    let (took, sent) = run(&start, WAIT, Vec::new()).await;
    assert!(took);
    assert_eq!(
        sent,
        vec![Command::SetOption {
            id: "mode".into(),
            value: OptionValue::Choice { id: "plan".into() },
        }]
    );
}

#[tokio::test]
async fn a_session_still_starting_is_asked_again() {
    let (took, sent) = run(&plan(), WAIT, vec![Some(Rejection::NotReady)]).await;
    assert!(took);
    assert_eq!(sent.len(), 2);
}

#[tokio::test]
async fn a_session_that_stays_down_past_the_wait_is_a_failure() {
    let (took, sent) = run(&plan(), Duration::ZERO, vec![Some(Rejection::NotReady)]).await;
    assert!(!took);
    assert_eq!(sent.len(), 1);
}

#[tokio::test]
async fn a_mode_the_agent_refuses_is_a_failure_without_asking_again() {
    let (took, sent) = run(&plan(), WAIT, vec![Some(Rejection::UnknownMode)]).await;
    assert!(!took);
    assert_eq!(sent.len(), 1);
}

#[tokio::test]
async fn nothing_to_start_with_takes_at_once() {
    let (took, sent) = run(&Start::default(), WAIT, Vec::new()).await;
    assert!(took);
    assert!(sent.is_empty());
}

#[test]
fn the_phones_fields_are_read() {
    let start: Start = serde_json::from_value(json!({
        "mode": "build", "option": "mode", "auto_accept": true
    }))
    .unwrap();
    assert_eq!(
        start,
        Start {
            auto_accept: true,
            mode: Some("build".into()),
            option: Some("mode".into()),
        }
    );
}
