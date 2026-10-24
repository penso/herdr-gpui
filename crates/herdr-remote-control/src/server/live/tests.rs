use super::*;
use crate::apns::{ContentState, Phase};

fn live(event: Event) -> Live {
    Live {
        event,
        pane: "w1-p3".to_owned(),
        state: ContentState {
            cost: None,
            phase: Phase::Finished,
            plan_done: None,
            plan_total: None,
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
