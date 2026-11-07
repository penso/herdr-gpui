#![allow(clippy::unwrap_used)]
use super::*;

fn response(next: Option<&str>) -> Value {
    json!({ "data": { "repository": { "pullRequests": {
        "pageInfo": { "hasNextPage": next.is_some(), "endCursor": next },
        "nodes": [
            {
                "number": 601,
                "reviewDecision": "CHANGES_REQUESTED",
                "mergeStateStatus": "DIRTY",
                "reviewRequests": { "nodes": [
                    { "requestedReviewer": { "__typename": "User", "login": "penso" } },
                    { "requestedReviewer": { "__typename": "Team", "slug": "core" } },
                    { "requestedReviewer": { "__typename": "Mannequin" } },
                    null
                ] },
                "commits": { "nodes": [ { "commit": { "statusCheckRollup": { "contexts": { "nodes": [
                    { "__typename": "CheckRun", "name": "macos", "status": "COMPLETED", "conclusion": "SUCCESS" },
                    { "__typename": "CheckRun", "name": "windows", "status": "COMPLETED", "conclusion": "FAILURE" },
                    { "__typename": "CheckRun", "name": "lint", "status": "IN_PROGRESS", "conclusion": null },
                    { "__typename": "CheckRun", "name": "docs", "status": "COMPLETED", "conclusion": "SKIPPED" },
                    { "__typename": "StatusContext", "context": "ci/legacy", "state": "PENDING" }
                ] } } } } ] }
            },
            {
                "number": 602,
                "reviewDecision": null,
                "reviewRequests": { "nodes": [] },
                "commits": { "nodes": [ { "commit": { "statusCheckRollup": null } } ] }
            }
        ]
    } } } })
}

#[test]
fn a_page_reads_reviews_reviewers_and_checks() {
    let page = parse(response(Some("cursor"))).unwrap();
    assert_eq!(page.next.as_deref(), Some("cursor"));
    let first = &page.statuses[&601];
    assert_eq!(first.review, Some(Review::ChangesRequested));
    assert_eq!(first.requested, ["penso", "team/core"]);
    assert!(first.requests("PENSO"));
    let states: Vec<_> = first.checks.iter().map(|check| check.state).collect();
    assert_eq!(
        states,
        [
            CheckState::Passed,
            CheckState::Failed,
            CheckState::Pending,
            CheckState::Skipped,
            CheckState::Pending,
        ]
    );
    // Any failure wins over what is still running.
    assert_eq!(first.checks_state(), Some(CheckState::Failed));
    // No checks and no verdict yet: nothing to say.
    let second = &page.statuses[&602];
    assert_eq!(second.review, None);
    assert_eq!(second.checks_state(), None);
    assert!(parse(response(None)).unwrap().next.is_none());
}

#[test]
fn a_page_reads_the_merge_state_and_a_missing_or_new_one_is_unknown() {
    let page = parse(response(None)).unwrap();
    assert_eq!(page.statuses[&601].merge, MergeState::Dirty);
    assert_eq!(page.statuses[&602].merge, MergeState::Unknown);
    let mut future = response(None);
    future["data"]["repository"]["pullRequests"]["nodes"][1]["mergeStateStatus"] = json!("NEW");
    assert_eq!(
        parse(future).unwrap().statuses[&602].merge,
        MergeState::Unknown
    );
}

#[test]
fn a_kept_status_round_trips_and_one_kept_before_the_merge_state_decodes() {
    let status = PrStatus {
        merge: MergeState::HasHooks,
        ..PrStatus::default()
    };
    let json = serde_json::to_string(&status).unwrap();
    assert_eq!(serde_json::from_str::<PrStatus>(&json).unwrap(), status);
    let old = r#"{"review":null,"requested":[],"checks":[]}"#;
    let old: PrStatus = serde_json::from_str(old).unwrap();
    assert_eq!(old.merge, MergeState::Unknown);
}

fn standing(review: Option<Review>, merge: MergeState, checks: &[CheckState]) -> PrStatus {
    PrStatus {
        review,
        merge,
        checks: checks
            .iter()
            .map(|state| Check {
                name: "ci".into(),
                state: *state,
            })
            .collect(),
        ..PrStatus::default()
    }
}

#[test]
fn a_status_takes_the_tone_of_the_badge_rule() {
    use CheckState::{Failed, Passed, Pending, Skipped};
    let tone = |review, merge, checks| standing(review, merge, checks).tone();
    assert_eq!(
        tone(None, MergeState::Clean, &[Passed, Skipped]),
        Tone::Ready
    );
    assert_eq!(
        tone(Some(Review::Approved), MergeState::Clean, &[Passed]),
        Tone::Ready
    );
    assert_eq!(tone(None, MergeState::Clean, &[Pending]), Tone::Waiting);
    assert_eq!(
        tone(Some(Review::Required), MergeState::Clean, &[Passed]),
        Tone::Waiting
    );
    // GitHub has not decided: never ready on that alone.
    assert_eq!(tone(None, MergeState::Unknown, &[Passed]), Tone::Waiting);
    assert_eq!(tone(None, MergeState::Behind, &[Passed]), Tone::Held);
    assert_eq!(tone(None, MergeState::Blocked, &[Passed]), Tone::Held);
    assert_eq!(tone(None, MergeState::Dirty, &[Passed]), Tone::Attention);
    assert_eq!(tone(None, MergeState::Clean, &[Failed]), Tone::Attention);
    assert_eq!(
        tone(Some(Review::ChangesRequested), MergeState::Clean, &[Passed]),
        Tone::Attention
    );
    // UNSTABLE while checks run is only checks running.
    assert_eq!(tone(None, MergeState::Unstable, &[Pending]), Tone::Waiting);
    assert_eq!(tone(None, MergeState::Unstable, &[Passed]), Tone::Attention);
}

#[test]
fn skipped_checks_count_as_fine_and_running_or_failed_ones_do_not() {
    use CheckState::{Failed, Passed, Pending, Skipped};
    let status = standing(None, MergeState::Clean, &[Passed, Skipped, Pending, Failed]);
    assert_eq!(status.checks_ok(), 2);
}

#[test]
fn reading_stops_at_the_last_page_or_the_page_limit() {
    let mut calls = 0;
    let statuses = fetch("sheprd-labs/sheprd", |_, variables| {
        calls += 1;
        // The second page is asked for with the first one's cursor.
        if calls == 2 {
            assert_eq!(variables["after"], "cursor");
        }
        Ok(response(Some("cursor")))
    })
    .unwrap();
    assert_eq!(calls, PAGES);
    assert_eq!(statuses.len(), 2);
    let mut calls = 0;
    fetch("sheprd-labs/sheprd", |_, _| {
        calls += 1;
        Ok(response(None))
    })
    .unwrap();
    assert_eq!(calls, 1);
    assert!(matches!(
        fetch("not-a-repository", |_, _| Ok(response(None))),
        Err(Error::RemoteUrl(_))
    ));
}
