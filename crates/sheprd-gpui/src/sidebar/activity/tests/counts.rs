use super::*;
use crate::sidebar::activity::counts::Counts;
use std::collections::HashSet;

#[test]
fn counts_open_pull_requests_and_open_issues_once() {
    let mut closed = item("3", "Closed issue");
    closed.state = "closed".into();
    let mut pull = item("pr/4", "Open pull request");
    pull.pull_request = Some(pull_request(4));
    let mut merged = item("pr/5", "Merged pull request");
    merged.pull_request = Some(pull_request(5));
    merged.state = "merged".into();
    let items = [
        item("1", "Open"),
        item("2", "Also open"),
        closed,
        pull,
        merged,
    ];
    let mut counts = Counts::default();
    let mut seen = HashSet::new();
    counts.add(&mut seen, &items);
    // The same repository in a second workspace's view counts once.
    counts.add(&mut seen, &items);
    assert_eq!(counts.of(ActivityPanel::PullRequests), Some(1));
    assert_eq!(counts.of(ActivityPanel::Issues), Some(2));
    assert_eq!(counts.of(ActivityPanel::Workspaces), None);
    assert_eq!(counts.of(ActivityPanel::Usage), None);
}
