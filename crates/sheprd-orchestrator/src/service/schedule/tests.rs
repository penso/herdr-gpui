#![allow(clippy::unwrap_used)]

use super::*;
use crate::{ItemKey, Provider, PullRequest, SourceKey, review::Check};

fn pull(number: u64, state: &str, head: &str, updated: DateTime<Utc>) -> Item {
    Item {
        key: ItemKey {
            source: SourceKey {
                provider: Provider::Github,
                host: "github.com".into(),
                repository: "penso/herdr-gpui".into(),
            },
            native_id: format!("pr/{number}"),
        },
        identifier: format!("#{number}"),
        title: String::new(),
        description: None,
        state: state.into(),
        url: None,
        author: None,
        labels: Vec::new(),
        parent_id: None,
        blocked_by: Vec::new(),
        assignee: None,
        acceptance: None,
        priority: None,
        created_at: None,
        updated_at: Some(updated),
        pull_request: Some(PullRequest {
            number,
            additions: None,
            deletions: None,
            base_ref: "main".into(),
            head_ref: "topic".into(),
            base_sha: String::new(),
            head_sha: head.into(),
            head_repository: None,
        }),
        activity: None,
    }
}

fn status(state: CheckState) -> PrStatus {
    PrStatus {
        checks: vec![Check {
            name: "ci".into(),
            state,
        }],
        ..PrStatus::default()
    }
}

#[test]
fn a_hidden_view_syncs_less_often_but_never_faster_than_asked() {
    let minute = Duration::from_secs(60);
    assert_eq!(period(minute, true), minute);
    assert_eq!(period(minute, false), HIDDEN_SYNC);
    let hour = Duration::from_secs(3600);
    assert_eq!(period(hour, false), hour);
}

#[test]
fn repeated_failures_double_up_to_the_ceiling() {
    let wait = Duration::from_secs(300);
    assert_eq!(backoff(wait, 1), wait);
    assert_eq!(backoff(wait, 2), Duration::from_secs(600));
    assert_eq!(backoff(wait, 3), Duration::from_secs(1200));
    assert_eq!(backoff(wait, 4), MAX_BACKOFF);
    assert_eq!(backoff(wait, u32::MAX), MAX_BACKOFF);
    // GitHub's own longer retry time is kept.
    let asked = Duration::from_secs(3000);
    assert_eq!(backoff(asked, 5), asked);
}

#[test]
fn only_open_pull_requests_are_compared() {
    let at = Utc::now();
    let open = open_pulls(&[
        pull(2, "draft", "b", at),
        pull(1, "open", "a", at),
        pull(3, "merged", "c", at),
    ]);
    assert_eq!(
        open,
        [(1, "a".to_owned(), Some(at)), (2, "b".to_owned(), Some(at))]
    );
}

#[test]
fn checks_are_read_again_only_when_they_may_have_moved() {
    let updated = Utc::now();
    let now = Instant::now();
    let open = open_pulls(&[pull(1, "open", "a", updated)]);
    let none = HashMap::new();
    assert!(checks_due(None, &open, "penso/herdr-gpui", &none, now));

    let fresh = ChecksRead {
        at: now,
        open: open.clone(),
    };
    assert!(!checks_due(
        Some(&fresh),
        &open,
        "penso/herdr-gpui",
        &none,
        now
    ));

    let pushed = open_pulls(&[pull(1, "open", "b", updated)]);
    assert!(checks_due(
        Some(&fresh),
        &pushed,
        "penso/herdr-gpui",
        &none,
        now
    ));

    let opened = open_pulls(&[pull(1, "open", "a", updated), pull(2, "open", "c", updated)]);
    assert!(checks_due(
        Some(&fresh),
        &opened,
        "penso/herdr-gpui",
        &none,
        now
    ));

    let running = HashMap::from([(
        ("penso/herdr-gpui".to_owned(), 1),
        status(CheckState::Pending),
    )]);
    assert!(checks_due(
        Some(&fresh),
        &open,
        "penso/herdr-gpui",
        &running,
        now
    ));
    assert!(
        !checks_due(Some(&fresh), &open, "penso/other", &running, now),
        "another repository's running checks"
    );
    let passed = HashMap::from([(
        ("penso/herdr-gpui".to_owned(), 1),
        status(CheckState::Passed),
    )]);
    assert!(!checks_due(
        Some(&fresh),
        &open,
        "penso/herdr-gpui",
        &passed,
        now
    ));

    let mut mixed = status(CheckState::Failed);
    mixed.checks.push(Check {
        name: "lint".into(),
        state: CheckState::Pending,
    });
    let failing_and_running = HashMap::from([(("penso/herdr-gpui".to_owned(), 1), mixed)]);
    assert!(
        checks_due(
            Some(&fresh),
            &open,
            "penso/herdr-gpui",
            &failing_and_running,
            now
        ),
        "a failed check does not hide one still running"
    );

    let later = now + CHECKS_EVERY;
    assert!(checks_due(
        Some(&fresh),
        &open,
        "penso/herdr-gpui",
        &none,
        later
    ));
}

#[test]
fn a_capped_listing_stays_capped_until_a_full_listing() {
    // Continued from a capped listing: its count alone proves nothing.
    assert!(!complete(true, false, Some(false)));
    assert!(
        complete(true, true, Some(false)),
        "a full listing read it all"
    );
    // Continued from a stored or complete listing, or from nothing.
    assert!(complete(true, false, Some(true)));
    assert!(complete(true, false, None));
    assert!(!complete(false, true, None));
    assert!(!complete(false, false, Some(true)));
}
