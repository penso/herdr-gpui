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
    // Both runs need the user: one waits for input, one failed.
    assert_eq!(attention(&runs), 2);
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
fn runs_group_by_what_needs_the_user_first() {
    let items = beads();
    let done = run("a", &items[0], RunState::Completed, "2026-10-04T03:00:00Z");
    let working = run("b", &items[1], RunState::Running, "2026-10-04T02:00:00Z");
    let waiting = run("c", &items[2], RunState::NeedsInput, "2026-10-04T01:00:00Z");
    let list = vec![done, working, waiting];
    let runs = Runs::new(&list, &[], &[]);
    let lines = run_lines(&runs, &items, "", false);
    let groups: Vec<_> = lines
        .iter()
        .map(|line| match line {
            RunLine::Group { group, count } => format!("{}:{count}", group.label()),
            RunLine::Run(row) => list[row.run].id.clone(),
        })
        .collect();
    assert_eq!(
        groups,
        ["NEEDS YOU:1", "c", "WORKING:1", "b", "FINISHED:1", "a"]
    );
    let active = run_lines(&runs, &items, "", true);
    assert_eq!(active.len(), 4);
    let searched = run_lines(&runs, &items, "store", false);
    assert_eq!(searched.len(), 2);
}

#[test]
fn pull_requests_list_in_every_state_unless_narrowed() {
    let mut open = item(Provider::Github, "pr/1", "Open one", "2026-10-04T00:00:00Z");
    let mut merged = item(
        Provider::Github,
        "pr/2",
        "Merged one",
        "2026-10-05T00:00:00Z",
    );
    for (pr, number) in [(&mut open, 1), (&mut merged, 2)] {
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
    }
    merged.state = "merged".into();
    merged.author = Some("someone".into());
    let items = vec![open, merged];
    let runs = Runs::new(&[], &[], &[]);
    let all = pull_request_rows(
        &items,
        &runs,
        "",
        Filters::default(),
        None,
        false,
        Sort::Updated,
    );
    assert_eq!(all.iter().map(|row| row.item).collect::<Vec<_>>(), [1, 0]);
    let open_only = Filters {
        open_only: true,
        ..Filters::default()
    };
    assert_eq!(
        pull_request_rows(&items, &runs, "", open_only, None, false, Sort::Updated).len(),
        1
    );
    let mine = pull_request_rows(
        &items,
        &runs,
        "",
        Filters::default(),
        Some("penso"),
        true,
        Sort::Updated,
    );
    assert_eq!(mine.iter().map(|row| row.item).collect::<Vec<_>>(), [0]);
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
