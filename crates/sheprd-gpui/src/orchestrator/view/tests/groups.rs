//! Section lists group their rows: issues by priority, children under their
//! parent, and pull requests by what each waits on.
use super::*;
use crate::orchestrator::review::Check;
use crate::orchestrator::{CheckState, PrStatus, PullRequest, Review};
use std::collections::{HashMap, HashSet};

fn headings(lines: &[Line], items: &[Item]) -> Vec<String> {
    lines
        .iter()
        .map(|line| match line {
            Line::Heading { heading, count } => format!("{heading:?}:{count}"),
            Line::Row(row) => items[row.item].identifier.clone(),
        })
        .collect()
}

#[test]
fn issues_group_by_priority_and_children_stay_under_their_parent() {
    let mut items = vec![
        item(Provider::Beads, "hg-1", "Epic", "2026-10-01T00:00:00Z"),
        item(Provider::Beads, "hg-1.1", "Child", "2026-10-03T00:00:00Z"),
        item(Provider::Github, "377", "Urgent", "2026-10-04T00:00:00Z"),
        item(Provider::Github, "378", "Someday", "2026-10-02T00:00:00Z"),
    ];
    items[0].priority = Some(2);
    // The child's own priority does not pull it away from its parent.
    items[1].priority = Some(0);
    items[1].parent_id = Some("hg-1".into());
    items[2].priority = Some(0);
    let runs = Runs::new(&[], &[], &[]);
    let rows = issue_rows(
        &items,
        &runs,
        "",
        Filters::default(),
        Sort::Updated,
        &HashSet::new(),
    );
    let lines = issue_lines(&rows, |row| &items[row.item]);
    assert_eq!(
        headings(&lines, &items),
        [
            "Priority(Some(0)):1",
            "377",
            "Priority(Some(2)):1",
            "hg-1",
            "hg-1.1",
            "Priority(None):1",
            "378",
        ]
    );
}

fn pull(number: u64, author: &str) -> Item {
    let mut pull = item(
        Provider::Github,
        &format!("pr/{number}"),
        "Change",
        "2026-10-04T00:00:00Z",
    );
    pull.identifier = format!("#{number}");
    pull.author = Some(author.into());
    pull.pull_request = Some(PullRequest {
        number,
        additions: None,
        deletions: None,
        base_ref: "main".into(),
        head_ref: format!("b-{number}"),
        base_sha: String::new(),
        head_sha: String::new(),
        head_repository: None,
    });
    pull
}

fn status(review: Option<Review>, requested: &[&str], checks: &[CheckState]) -> PrStatus {
    PrStatus {
        review,
        requested: requested.iter().map(|login| (*login).to_owned()).collect(),
        checks: checks
            .iter()
            .enumerate()
            .map(|(index, state)| Check {
                name: format!("check-{index}"),
                state: *state,
            })
            .collect(),
    }
}

#[test]
fn pull_requests_group_by_what_each_waits_on() {
    let items = vec![
        pull(1, "leo"),
        pull(2, "penso"),
        pull(3, "penso"),
        pull(4, "leo"),
        {
            let mut merged = pull(5, "penso");
            merged.state = "merged".into();
            merged
        },
    ];
    let repository = items[0].key.source.repository.clone();
    let statuses: HashMap<(String, u64), PrStatus> = [
        // Someone else's, with the user asked to review.
        (
            1,
            status(Some(Review::Required), &["penso"], &[CheckState::Passed]),
        ),
        // The user's own, failing CI.
        (
            2,
            status(None, &[], &[CheckState::Passed, CheckState::Failed]),
        ),
        // The user's own, approved and green.
        (
            3,
            status(Some(Review::Approved), &[], &[CheckState::Passed]),
        ),
        // Someone else's, checks still running.
        (
            4,
            status(Some(Review::Approved), &[], &[CheckState::Pending]),
        ),
    ]
    .into_iter()
    .map(|(number, status)| ((repository.clone(), number), status))
    .collect();
    let rows: Vec<ItemRow> = (0..items.len())
        .map(|item| ItemRow {
            repo: 0,
            item,
            depth: 0,
            children: None,
            run: None,
        })
        .collect();
    let next = |login: Option<&'static str>| {
        let (items, statuses) = (&items, &statuses);
        move |row: &ItemRow| {
            let item = &items[row.item];
            Next::of(item, status_of(item, statuses), login)
        }
    };
    let lines = pull_request_lines(&rows, next(Some("Penso")));
    assert_eq!(
        headings(&lines, &items),
        [
            "Next(Review):1",
            "#1",
            "Next(Fix):1",
            "#2",
            "Next(Merge):1",
            "#3",
            "Next(Wait):1",
            "#4",
            "Next(Closed):1",
            "#5",
        ]
    );
    // Signed out, nothing is the user's: all open ones wait or can merge.
    let lines = pull_request_lines(&rows, next(None));
    assert!(
        !headings(&lines, &items)
            .iter()
            .any(|line| line.starts_with("Next(Review)"))
    );
}

#[test]
fn rows_of_several_repositories_merge_in_sort_order_keeping_children() {
    let first = vec![
        item(Provider::Beads, "hg-1", "Epic", "2026-10-01T00:00:00Z"),
        item(Provider::Beads, "hg-1.1", "Child", "2026-10-09T00:00:00Z"),
    ];
    let second = vec![item(
        Provider::Github,
        "377",
        "Newer",
        "2026-10-05T00:00:00Z",
    )];
    let row = |repo: usize, item: usize, depth: u8| ItemRow {
        repo,
        item,
        depth,
        children: None,
        run: None,
    };
    let repos = [&first, &second];
    let merged = merge(
        [vec![row(0, 0, 0), row(0, 1, 1)], vec![row(1, 0, 0)]],
        |row| &repos[row.repo][row.item],
        Sort::Updated,
    );
    // The newer top-level row leads; the child stays under its parent even
    // though it is newer still.
    let order: Vec<_> = merged
        .iter()
        .map(|row| repos[row.repo][row.item].identifier.as_str())
        .collect();
    assert_eq!(order, ["377", "hg-1", "hg-1.1"]);
}
