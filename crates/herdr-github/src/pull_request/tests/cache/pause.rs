use super::*;
use crate::pull_request::cache::paused;
use chrono::{Local, TimeZone};

#[test]
fn a_pause_names_its_cause_and_the_minute_it_ends() {
    let now = Local.with_ymd_and_hms(2026, 10, 13, 13, 24, 30).unwrap();
    assert_eq!(
        paused(
            Duration::from_secs(90),
            Some("GitHub rate limit reached. Retry later."),
            now
        ),
        "GitHub rate limit reached. Retry later. GitHub requests paused; retrying at 13:26."
    );
    // Part of a minute rounds up, so the shown time is never early.
    assert!(paused(Duration::from_secs(91), None, now).ends_with("retrying at 13:27."));
}

#[test]
fn a_paused_account_shows_why_until_it_resumes() {
    let mut peer = Peer::new();
    let now = Instant::now();
    peer.cache
        .schedule([input("limited"), input("waiting")], now);
    peer.cache.poll(now);
    peer.complete(
        now,
        Err(Error::RateLimit { reset: None }),
        Some(Duration::from_secs(600)),
    );
    let mut view = Lookup::default();
    peer.cache.present(&input("waiting"), &mut view, now);
    let message = view.message.clone().unwrap();
    assert!(
        message.starts_with("GitHub rate limit reached."),
        "{message}"
    );
    assert!(message.contains("retrying at "), "{message}");
}

#[test]
fn a_stale_result_keeps_the_reason_its_refresh_failed() {
    let mut peer = Peer::new();
    let now = Instant::now();
    let stale = input("stale");
    peer.cache.seed(stale.clone(), fixture().unwrap(), now);
    peer.cache.refresh(stale.clone(), now);
    peer.cache.poll(now);
    peer.complete(
        now,
        Err(Error::RepositoryHidden {
            repository: "acme/hidden".into(),
            app_token: true,
        }),
        None,
    );
    let mut view = Lookup::default();
    peer.cache.present(&stale, &mut view, now);
    assert!(view.value.is_some());
    let message = view.message.clone().unwrap();
    assert!(
        message.starts_with(
            "Refresh unavailable, showing the last result. GitHub cannot see acme/hidden"
        ),
        "{message}"
    );
}
