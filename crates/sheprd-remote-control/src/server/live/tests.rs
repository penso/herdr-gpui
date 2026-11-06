use super::*;
use crate::apns::{ContentState, Phase};

fn live(event: Event) -> Live {
    Live {
        event,
        pane: "w1-p3".to_owned(),
        state: ContentState {
            options: Vec::new(),
            permission: None,
            phase: Phase::Finished,
            plan_done: None,
            plan_total: None,
            previous: Vec::new(),
            question: None,
            started_at: 0,
            step: String::new(),
            summary: None,
            waiting: 0,
            working: 0,
        },
    }
}

fn registration(json: &str) -> Registration {
    Registration::parse(json.as_bytes()).unwrap()
}

#[test]
fn an_end_waits_for_the_activity_token_on_a_device_that_starts_activities() {
    let start_only = registration(&format!(r#"{{"push_to_start":"{}"}}"#, "ab".repeat(32)));
    let end = live(Event::End { dismissal_s: 0 });
    // The activity started, but its own token has not arrived yet.
    assert_eq!(route(&end, &start_only), Route::Hold);
    let update = live(Event::Update { alert: None });
    assert_eq!(route(&update, &start_only), Route::Hold);

    let with_activity = registration(&format!(
        r#"{{"push_to_start":"{}","activities":[{{"pane":"w1-p3","token":"{}"}}]}}"#,
        "ab".repeat(32),
        "cd".repeat(40)
    ));
    let Route::Send(token) = route(&end, &with_activity) else {
        panic!("expected a send");
    };
    assert_eq!(token.as_str(), "cd".repeat(40));

    // A device that cannot start activities shows none to update.
    assert_eq!(route(&end, &Registration::default()), Route::Skip);
}

mod detail {
    use super::*;
    use crate::app::config::ActivityDetail;

    const STEP: &str = "Editing secret.rs";

    fn question() -> Live {
        let mut live = live(Event::Update { alert: None });
        live.state.phase = Phase::Question;
        live.state.step = STEP.to_owned();
        live
    }

    #[test]
    fn a_phone_can_ask_for_less_than_its_computer_allows() {
        let dir = tempfile::tempdir().unwrap();
        let state = crate::web::tests::state(dir.path());
        let generic = registration(r#"{"activity_detail":"generic"}"#);
        assert_ne!(shown(&state, question(), &generic).state.step, STEP);
        let detailed = Registration::default();
        assert_eq!(shown(&state, question(), &detailed).state.step, STEP);
    }

    #[test]
    fn a_phone_cannot_ask_for_more_than_its_computer_allows() {
        let dir = tempfile::tempdir().unwrap();
        let mut state = crate::web::tests::state(dir.path());
        Arc::get_mut(&mut state).unwrap().live_activities = ActivityDetail::Generic;
        let detailed = Registration::default();
        assert_ne!(shown(&state, question(), &detailed).state.step, STEP);
    }
}
