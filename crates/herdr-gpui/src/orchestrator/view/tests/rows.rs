use super::*;
use std::collections::HashSet;

fn beads() -> Vec<Item> {
    let mut epic = item(Provider::Beads, "hg-1", "Epic", "2026-10-01T00:00:00Z");
    epic.priority = Some(1);
    let mut child = item(Provider::Beads, "hg-1.1", "Store", "2026-10-03T00:00:00Z");
    child.parent_id = Some("hg-1".into());
    let mut blocked = item(
        Provider::Beads,
        "hg-1.2",
        "Over SSH",
        "2026-10-02T00:00:00Z",
    );
    blocked.parent_id = Some("hg-1".into());
    blocked.blocked_by = vec!["hg-1.1".into()];
    let mut closed = item(
        Provider::Beads,
        "hg-2",
        "Done thing",
        "2026-10-05T00:00:00Z",
    );
    closed.state = "closed".into();
    let issue = item(
        Provider::Github,
        "377",
        "Prompt icons render as tofu",
        "2026-10-04T00:00:00Z",
    );
    vec![epic, child, blocked, closed, issue]
}

fn ids(items: &[Item], rows: &[ItemRow]) -> Vec<(String, u8)> {
    rows.iter()
        .map(|row| (items[row.item].identifier.clone(), row.depth))
        .collect()
}

#[test]
fn beads_children_sit_under_their_parent_and_closed_items_hide() {
    let items = beads();
    let runs = Runs::new(&[], &[], &[]);
    let rows = issue_rows(
        &items,
        &runs,
        "",
        Filters::default(),
        Sort::Updated,
        &HashSet::new(),
    );
    assert_eq!(
        ids(&items, &rows),
        [
            ("377".into(), 0),
            ("hg-1".into(), 0),
            ("hg-1.1".into(), 1),
            ("hg-1.2".into(), 1)
        ]
    );
    assert_eq!(rows[1].children, Some(true));

    let collapsed = HashSet::from([items[0].key.canonical()]);
    let rows = issue_rows(
        &items,
        &runs,
        "",
        Filters::default(),
        Sort::Updated,
        &collapsed,
    );
    assert_eq!(ids(&items, &rows), [("377".into(), 0), ("hg-1".into(), 0)]);
    assert_eq!(rows[1].children, Some(false));
}

#[test]
fn a_search_keeps_the_ancestors_of_a_match_and_opens_them() {
    let items = beads();
    let runs = Runs::new(&[], &[], &[]);
    let collapsed = HashSet::from([items[0].key.canonical()]);
    let rows = issue_rows(
        &items,
        &runs,
        "ssh",
        Filters::default(),
        Sort::Updated,
        &collapsed,
    );
    assert_eq!(
        ids(&items, &rows),
        [("hg-1".into(), 0), ("hg-1.2".into(), 1)]
    );
    // A capital makes the word case-sensitive.
    let rows = issue_rows(
        &items,
        &runs,
        "SSH",
        Filters::default(),
        Sort::Updated,
        &collapsed,
    );
    assert_eq!(
        ids(&items, &rows),
        [("hg-1".into(), 0), ("hg-1.2".into(), 1)]
    );
    let rows = issue_rows(
        &items,
        &runs,
        "Ssh",
        Filters::default(),
        Sort::Updated,
        &collapsed,
    );
    assert!(rows.is_empty());
}

#[test]
fn filters_and_sorts_apply() {
    let items = beads();
    let runs = Runs::new(&[], &[], &[]);
    let blocked = Filters {
        blocked: true,
        ..Filters::default()
    };
    let rows = issue_rows(&items, &runs, "", blocked, Sort::Updated, &HashSet::new());
    assert_eq!(
        ids(&items, &rows),
        [("hg-1".into(), 0), ("hg-1.2".into(), 1)]
    );
    let github = Filters {
        provider: Some(Provider::Github),
        ..Filters::default()
    };
    let rows = issue_rows(&items, &runs, "", github, Sort::Updated, &HashSet::new());
    assert_eq!(ids(&items, &rows), [("377".into(), 0)]);
    let rows = issue_rows(
        &items,
        &runs,
        "",
        Filters::default(),
        Sort::Priority,
        &HashSet::new(),
    );
    assert_eq!(rows[0].item, 0, "P1 comes before items without a priority");
}

#[test]
fn live_herdr_status_overrides_the_stored_state() {
    let items = beads();
    let older = run("a", &items[4], RunState::Failed, "2026-10-04T01:00:00Z");
    let newer = run("b", &items[4], RunState::Running, "2026-10-04T02:00:00Z");
    let runs_list = vec![older.clone(), newer.clone()];
    let sessions = vec![session(&newer, "p9")];
    let agents = vec![live(&newer, "p9", AgentStatus::Blocked)];
    let runs = Runs::new(&runs_list, &sessions, &agents);
    assert_eq!(runs.status(1), Some(Status::NeedsInput));
    assert_eq!(runs.status(0), Some(Status::Failed));
    let rows = issue_rows(
        &items,
        &runs,
        "",
        Filters::default(),
        Sort::Updated,
        &HashSet::new(),
    );
    assert_eq!(
        rows[0].run,
        Some(RunRow {
            run: 1,
            status: Status::NeedsInput
        })
    );
    // An agent on another host with the same pane id is not this run's.
    let elsewhere = vec![LiveAgent {
        host: Some("devbox".into()),
        ..live(&newer, "p9", AgentStatus::Working)
    }];
    let runs = Runs::new(&runs_list, &sessions, &elsewhere);
    assert!(runs.live(1).is_none());
    // Without its agent in view, the run shows the state its owner stored.
    assert_eq!(runs.status(1), Some(Status::Working));
}

#[test]
fn pull_requests_hide_merged_and_closed_until_asked_for() {
    let states = [
        ("open", "2026-10-04T00:00:00Z"),
        ("draft", "2026-10-03T00:00:00Z"),
        ("merged", "2026-10-05T00:00:00Z"),
        ("closed", "2026-10-06T00:00:00Z"),
    ];
    let items: Vec<Item> = states
        .into_iter()
        .zip(1..)
        .map(|((state, updated), number)| {
            let mut pr = item(Provider::Github, &format!("pr/{number}"), state, updated);
            pr.state = state.into();
            if state != "open" {
                pr.author = Some("someone".into());
            }
            pr.pull_request = Some(crate::orchestrator::PullRequest {
                number,
                additions: None,
                deletions: None,
                base_ref: "main".into(),
                head_ref: "x".into(),
                base_sha: String::new(),
                head_sha: String::new(),
                head_repository: None,
            });
            pr
        })
        .collect();
    let runs = Runs::new(&[], &[], &[]);
    let statuses = std::collections::HashMap::new();
    let listed = |filters: Filters, mine: bool| {
        pull_request_rows(
            &items,
            &runs,
            "",
            filters,
            (Some("penso"), &statuses),
            Yours {
                authored: mine,
                review_requested: false,
            },
            Sort::Updated,
        )
        .iter()
        .map(|row| row.item)
        .collect::<Vec<_>>()
    };
    assert_eq!(listed(Filters::default(), false), [0, 1]);
    let every_state = Filters {
        open_only: false,
        ..Filters::default()
    };
    assert_eq!(listed(every_state, false), [3, 2, 0, 1]);
    assert_eq!(listed(every_state, true), [0]);
}

#[test]
fn pull_requests_open_on_their_conversation_and_have_checks() {
    use crate::orchestrator::view::DetailTab;
    assert_eq!(DetailTab::tabs(true)[0], DetailTab::Conversation);
    assert!(DetailTab::tabs(true).contains(&DetailTab::Checks));
    assert!(!DetailTab::tabs(false).contains(&DetailTab::Checks));
}

#[test]
fn a_run_without_a_session_is_found_by_its_workspace() {
    // agent-launcher builds before the shared sessions table record only the
    // run's Herdr workspace.
    let items = beads();
    let theirs = run("old", &items[4], RunState::Running, "2026-10-04T01:00:00Z");
    let list = vec![theirs.clone()];
    let agents = vec![live(&theirs, "p3", AgentStatus::Blocked)];
    let runs = Runs::new(&list, &[], &agents);
    assert_eq!(runs.live(0).map(|agent| agent.pane_id.as_str()), Some("p3"));
    assert_eq!(runs.status(0), Some(Status::NeedsInput));
    // The same workspace id on another host is another workspace.
    let elsewhere = vec![LiveAgent {
        host: Some("devbox".into()),
        ..live(&theirs, "p3", AgentStatus::Blocked)
    }];
    let runs = Runs::new(&list, &[], &elsewhere);
    assert!(runs.live(0).is_none());
    assert_eq!(runs.status(0), Some(Status::Working));
}

#[test]
fn unlisted_items_are_named_by_their_key() {
    assert_eq!(key_label("github:github.com:penso/herdr-gpui:375"), "#375");
    assert_eq!(
        key_label("github:github.com:penso/herdr-gpui:pr/369"),
        "#369"
    );
    assert_eq!(
        key_label("beads:local:/Users/me/app:hg-a3f2.1"),
        "hg-a3f2.1"
    );
    assert_eq!(key_label("beads:local:/r:a%3Ab"), "a:b");
}
