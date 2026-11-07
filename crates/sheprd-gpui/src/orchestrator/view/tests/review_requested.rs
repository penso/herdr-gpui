use super::*;
use crate::orchestrator::PrStatus;
use std::collections::HashMap;

/// Three open pull requests in `penso/app`: #1 opened by the signed-in
/// account, #2 by someone else asking it for a review, #3 by someone else.
fn pull_requests() -> (Vec<Item>, HashMap<(String, u64), PrStatus>) {
    let items = [("Penso", 1), ("someone", 2), ("someone", 3)]
        .into_iter()
        .map(|(author, number)| {
            let mut pr = item(
                Provider::Github,
                &format!("pr/{number}"),
                "fixture",
                &format!("2026-10-0{number}T00:00:00Z"),
            );
            pr.author = Some(author.into());
            pr.pull_request = Some(crate::orchestrator::PullRequest {
                number,
                additions: None,
                deletions: None,
                base_ref: "main".into(),
                head_ref: format!("topic-{number}"),
                base_sha: String::new(),
                head_sha: String::new(),
                head_repository: None,
            });
            pr
        })
        .collect();
    let asked = PrStatus {
        requested: vec!["PENSO".into(), "team/core".into()],
        ..PrStatus::default()
    };
    let other = PrStatus {
        requested: vec!["someone-else".into()],
        ..PrStatus::default()
    };
    let statuses = HashMap::from([
        (("penso/app".to_owned(), 2), asked),
        (("penso/app".to_owned(), 3), other),
    ]);
    (items, statuses)
}

fn listed(login: Option<&str>, authored: bool, review_requested: bool) -> Vec<u64> {
    let (items, statuses) = pull_requests();
    let runs = Runs::new(&[], &[], &[]);
    let mut numbers: Vec<u64> = pull_request_rows(
        &items,
        &runs,
        "",
        Filters::default(),
        (login, &statuses),
        Yours {
            authored,
            review_requested,
        },
        Sort::Updated,
    )
    .iter()
    .filter_map(|row| items[row.item].github_pull_request().map(|pr| pr.number))
    .collect();
    numbers.sort_unstable();
    numbers
}

#[test]
fn review_requested_lists_the_pull_requests_waiting_on_the_account() {
    assert_eq!(listed(Some("penso"), false, false), [1, 2, 3]);
    assert_eq!(
        listed(Some("penso"), true, false),
        [1],
        "logins ignore case"
    );
    assert_eq!(listed(Some("penso"), false, true), [2]);
    // Both toggles list either kind rather than only pull requests of both.
    assert_eq!(listed(Some("penso"), true, true), [1, 2]);
}

#[test]
fn without_a_login_nothing_is_the_accounts() {
    assert_eq!(listed(None, false, false), [1, 2, 3]);
    assert!(listed(None, true, false).is_empty());
    assert!(listed(None, false, true).is_empty());
}
