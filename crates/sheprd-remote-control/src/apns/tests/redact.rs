use super::*;
use crate::{app::config::ActivityDetail, push::Privacy};
use sheprd_chat_protocol::{PermissionKind, PermissionOption};

fn state(phase: Phase) -> ContentState {
    ContentState {
        options: vec![PermissionOption {
            id: "allow".to_owned(),
            kind: PermissionKind::AllowOnce,
            name: "Allow once".to_owned(),
        }],
        permission: Some(7),
        phase,
        plan_done: Some(2),
        plan_total: Some(4),
        previous: vec!["Read secret.rs".to_owned()],
        question: Some("Run git push origin secret?".to_owned()),
        started_at: 1_800_000_000,
        step: "Editing secret.rs".to_owned(),
        summary: Some("Failed: secret.rs is broken".to_owned()),
        waiting: 1,
        working: 2,
    }
}

fn start() -> Live {
    Live {
        event: Event::Start {
            alert: Alert {
                body: "secret-workspace · secret chat".to_owned(),
                title: "Claude is working".to_owned(),
            },
            attributes: Attributes {
                agent: "Claude".to_owned(),
                chat_title: "secret chat".to_owned(),
                computer: "studio".to_owned(),
                computer_id: "AB".repeat(32),
                pane: "w1-p3".to_owned(),
                workspace: "secret-workspace".to_owned(),
            },
        },
        pane: "w1-p3".to_owned(),
        state: state(Phase::Question),
    }
}

fn update(phase: Phase, alert: Option<Alert>) -> Live {
    Live {
        event: Event::Update { alert },
        pane: "w1-p3".to_owned(),
        state: state(phase),
    }
}

#[test]
fn excerpt_and_detailed_change_nothing() {
    let live = start();
    assert_eq!(
        live.clone()
            .redacted(Privacy::Excerpt, ActivityDetail::Detailed),
        live
    );
}

#[test]
fn generic_notifications_keep_the_chat_but_not_its_words() {
    let live = start().redacted(Privacy::Generic, ActivityDetail::Detailed);
    assert_eq!(live.state.question, None);
    assert_eq!(live.state.step, "Editing secret.rs");
    assert_eq!(live.state.previous, ["Read secret.rs"]);
    let Event::Start { attributes, .. } = &live.event else {
        panic!("expected a start");
    };
    assert_eq!(attributes.chat_title, "secret chat");
}

#[test]
fn buttons_never_outlive_the_question_they_answer() {
    for (privacy, detail) in [
        (Privacy::Generic, ActivityDetail::Detailed),
        (Privacy::Excerpt, ActivityDetail::Generic),
    ] {
        let state = start().redacted(privacy, detail).state;
        assert_eq!(state.question, None);
        assert_eq!(state.permission, None);
        assert!(state.options.is_empty());
    }
}

#[test]
fn a_failure_message_may_quote_the_chat_but_usage_does_not() {
    let failed = update(Phase::Failed, None).redacted(Privacy::Generic, ActivityDetail::Detailed);
    assert_eq!(failed.state.summary, None);
    let mut finished = update(Phase::Finished, None);
    finished.state.summary = Some("Worked 12s".to_owned());
    let finished = finished.redacted(Privacy::Generic, ActivityDetail::Detailed);
    assert_eq!(finished.state.summary.as_deref(), Some("Worked 12s"));
}

#[test]
fn a_generic_payload_says_nothing_about_the_chat() {
    let alert = Alert {
        body: "secret-workspace · secret chat".to_owned(),
        title: "Claude needs an answer".to_owned(),
    };
    let finished = {
        let mut live = update(Phase::Finished, None);
        live.event = Event::End { dismissal_s: 1 };
        live
    };
    for live in [
        start(),
        update(Phase::Working, None),
        update(Phase::Question, Some(alert)),
        update(Phase::Failed, None),
        finished,
    ] {
        for privacy in [Privacy::Generic, Privacy::Excerpt] {
            let shown = live.clone().redacted(privacy, ActivityDetail::Generic);
            let payload = String::from_utf8(shown.payload(1).unwrap()).unwrap();
            assert!(!payload.contains("secret"), "{payload}");
        }
    }
}

#[test]
fn a_generic_activity_keeps_what_the_app_needs() {
    let live = start().redacted(Privacy::Generic, ActivityDetail::Generic);
    assert_eq!(live.state.phase, Phase::Question);
    assert_eq!(
        (live.state.plan_done, live.state.plan_total),
        (Some(2), Some(4))
    );
    assert_eq!((live.state.waiting, live.state.working), (1, 2));
    let Event::Start { attributes, .. } = &live.event else {
        panic!("expected a start");
    };
    assert_eq!(attributes.agent, "Claude");
    assert_eq!(attributes.pane, "w1-p3");
    assert_eq!(attributes.computer_id, "AB".repeat(32));
    // The banner's place line shows the computer rather than nothing.
    assert_eq!(attributes.workspace, "studio");
    assert_eq!(attributes.chat_title, "");
}

#[test]
fn the_more_private_choice_wins() {
    use ActivityDetail::{Detailed, Generic};
    assert_eq!(Detailed.stricter(Detailed), Detailed);
    assert_eq!(Detailed.stricter(Generic), Generic);
    assert_eq!(Generic.stricter(Detailed), Generic);
    assert_eq!(Generic.stricter(Generic), Generic);
}
