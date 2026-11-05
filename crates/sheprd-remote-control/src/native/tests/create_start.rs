//! What a new chat starts with: the mode goes first, a session still
//! starting is asked again, and a mode counts only once the transcript
//! shows it. Anything less is reported, so the first prompt waits, and a
//! chat without its mode does not auto-accept.

use crate::hub;
use crate::native::create::{Start, apply_start};
use serde_json::json;
use sheprd_chat_protocol::{
    Choice, Command, ConfigOption, Mode, Modes, OptionCategory, OptionKind, OptionValue, Rejection,
    Transcript,
};
use std::{cell::RefCell, collections::VecDeque, future::ready, time::Duration};

const WAIT: Duration = Duration::from_secs(5);

/// Runs `apply_start` against answers given in order, then `Ok(None)`, for
/// an agent that goes on to apply what it was sent, and returns whether it
/// took and what was sent.
async fn run(
    start: &Start,
    wait: Duration,
    answers: Vec<Option<Rejection>>,
) -> (bool, Vec<Command>) {
    run_shown(start, wait, answers, || true).await
}

/// [`run`], with `shown` saying what the transcript shows.
async fn run_shown(
    start: &Start,
    wait: Duration,
    answers: Vec<Option<Rejection>>,
    shown: impl FnMut() -> bool,
) -> (bool, Vec<Command>) {
    run_answers(start, wait, answers.into_iter().map(Ok).collect(), shown).await
}

/// [`run_shown`], with what the chat answers, failures included.
async fn run_answers(
    start: &Start,
    wait: Duration,
    answers: Vec<hub::Result<Option<Rejection>>>,
    shown: impl FnMut() -> bool,
) -> (bool, Vec<Command>) {
    let answers = RefCell::new(VecDeque::from(answers));
    let sent = RefCell::new(Vec::new());
    let took = apply_start(
        start,
        wait,
        |command| {
            sent.borrow_mut().push(command);
            ready(answers.borrow_mut().pop_front().unwrap_or(Ok(None)))
        },
        shown,
    )
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
async fn a_connection_still_coming_up_is_asked_again() {
    let answers = vec![Err(hub::Error::ChatUnavailable)];
    let (took, sent) = run_answers(&plan(), WAIT, answers, || true).await;
    assert!(took);
    assert_eq!(sent.len(), 2);
}

#[tokio::test]
async fn auto_accepting_alone_waits_for_the_connection_too() {
    let start = Start {
        auto_accept: true,
        ..Start::default()
    };
    let answers = vec![Err(hub::Error::ChatUnavailable)];
    let (took, sent) = run_answers(&start, WAIT, answers, || true).await;
    assert!(took);
    assert_eq!(sent.len(), 2);
}

#[tokio::test]
async fn a_connection_that_stays_down_past_the_wait_is_a_failure() {
    let answers = vec![Err(hub::Error::ChatUnavailable)];
    let (took, sent) = run_answers(&plan(), Duration::ZERO, answers, || true).await;
    assert!(!took);
    assert_eq!(sent.len(), 1);
}

#[tokio::test]
async fn any_other_failure_is_not_asked_again() {
    let answers = vec![Err(hub::Error::Busy)];
    let (took, sent) = run_answers(&plan(), WAIT, answers, || true).await;
    assert!(!took);
    assert_eq!(sent.len(), 1);
}

#[tokio::test]
async fn a_refused_mode_does_not_turn_auto_accepting_on() {
    let start = Start {
        auto_accept: true,
        ..plan()
    };
    let (took, sent) = run(&start, WAIT, vec![Some(Rejection::UnknownMode)]).await;
    assert!(!took);
    assert_eq!(sent, vec![Command::SetMode { id: "plan".into() }]);
}

#[tokio::test]
async fn an_acknowledged_mode_the_agent_never_shows_is_a_failure() {
    let start = Start {
        auto_accept: true,
        ..plan()
    };
    let (took, sent) = run_shown(&start, Duration::ZERO, Vec::new(), || false).await;
    assert!(!took);
    assert_eq!(sent, vec![Command::SetMode { id: "plan".into() }]);
}

#[tokio::test]
async fn a_mode_that_shows_after_its_acknowledgement_takes() {
    let mut checks = 0;
    let shown = || {
        checks += 1;
        checks > 1
    };
    let (took, _) = run_shown(&plan(), WAIT, Vec::new(), shown).await;
    assert!(took);
}

fn with_modes(current: &str) -> Transcript {
    let mut transcript = Transcript::new("Claude");
    transcript.modes = Some(Modes {
        available: vec![Mode {
            description: None,
            id: "plan".into(),
            name: "Plan".into(),
        }],
        current: current.into(),
    });
    transcript
}

fn with_option(current: &str) -> Transcript {
    let mut transcript = Transcript::new("OpenCode");
    transcript.options = vec![ConfigOption {
        category: OptionCategory::Mode,
        description: None,
        id: "mode".into(),
        kind: OptionKind::Select {
            choices: vec![Choice {
                description: None,
                group: None,
                id: "plan".into(),
                name: "Plan".into(),
            }],
            current: current.into(),
        },
        name: "Mode".into(),
    }];
    transcript
}

#[test]
fn the_transcript_shows_a_mode_only_when_it_is_the_current_one() {
    assert!(plan().shown_in(&with_modes("plan")));
    assert!(!plan().shown_in(&with_modes("default")));
    assert!(!plan().shown_in(&Transcript::new("Claude")));
    let option = Start {
        option: Some("mode".into()),
        ..plan()
    };
    assert!(option.shown_in(&with_option("plan")));
    assert!(!option.shown_in(&with_option("build")));
    assert!(!option.shown_in(&with_modes("plan")));
    assert!(Start::default().shown_in(&Transcript::new("Claude")));
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
