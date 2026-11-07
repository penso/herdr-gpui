use super::*;
use crate::control::app_update::Stage;

const RUN: u32 = 7;

fn available() -> AppUpdate {
    AppUpdate {
        stage: Stage::Available,
        latest: Some("20261106.1".into()),
        detail: None,
    }
}

#[test]
fn a_request_needs_an_app_reporting() {
    let mut relay = Relay::default();
    assert!(matches!(relay.ask(Instant::now()), Err(Error::AppClosed)));
}

#[test]
fn the_next_report_takes_a_request_once() {
    let now = Instant::now();
    let mut relay = Relay::default();
    assert!(!relay.report(RUN, available(), now));
    relay.ask(now).unwrap();
    assert!(relay.status(now).asked);
    assert!(relay.report(RUN, available(), now));
    assert!(!relay.report(RUN, available(), now));
    assert!(!relay.status(now).asked);
}

#[test]
fn an_app_gone_silent_counts_as_closed() {
    let now = Instant::now();
    let mut relay = Relay::default();
    relay.report(RUN, available(), now);
    relay.ask(now).unwrap();
    let later = now + APP_SILENCE;
    assert_eq!(
        relay.status(later),
        Status {
            app: None,
            asked: false
        }
    );
    assert!(matches!(relay.ask(later), Err(Error::AppClosed)));
}

#[test]
fn a_request_does_not_outlive_the_app_it_was_made_to() {
    let now = Instant::now();
    let mut relay = Relay::default();
    relay.report(RUN, available(), now);
    relay.ask(now).unwrap();
    assert!(!relay.report(RUN, available(), now + APP_SILENCE));
    assert!(!relay.report(RUN, available(), now + APP_SILENCE));
}

#[test]
fn a_request_is_not_handed_to_an_app_reopened_at_once() {
    let now = Instant::now();
    let mut relay = Relay::default();
    relay.report(RUN, available(), now);
    relay.ask(now).unwrap();
    assert!(!relay.report(RUN + 1, available(), now));
    assert!(!relay.status(now).asked);
    assert!(!relay.report(RUN, available(), now));
}

#[test]
fn a_report_too_long_for_a_phone_is_invalid() {
    let mut update = available();
    assert!(update.valid());
    update.detail = Some("x".repeat(crate::control::app_update::MAX_UPDATE_TEXT + 1));
    assert!(!update.valid());
}
