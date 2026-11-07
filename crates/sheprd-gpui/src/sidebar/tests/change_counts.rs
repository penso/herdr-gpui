use super::*;
use crate::{
    config::ChangeCounts,
    git::{Ahead, Counted, Git, Status},
    sidebar::workspaces::workspace_badge,
};
use std::time::Instant;

fn input(branch: &str) -> crate::pull_request::Input {
    crate::pull_request::Input {
        checkout: None,
        repo_key: Some(layout_tests::REPO_KEY.into()),
        branch: branch.into(),
    }
}

/// The row's counts as drawn, or `None` when it shows none.
fn counts(
    workspace: &ClientShellWorkspace,
    cache: &crate::pull_request::Cache,
    git: &Git,
    mode: ChangeCounts,
) -> Option<(String, String)> {
    let marks = crate::teleport::Marks::detached();
    workspace_badge(
        workspace,
        Some((cache, &crate::pull_request::View::default())),
        (Some(git), mode),
        (&marks, crate::endpoint::LOCAL),
        (None, false),
        &Theme::default(),
    )?
    .counts
    .map(|counts| (counts.additions, counts.deletions))
}

#[test]
fn a_row_shows_only_the_change_counts_the_setting_picks() {
    let snapshot = layout_tests::snapshot(6);
    let (child, head) = (&snapshot.workspaces[4], &snapshot.workspaces[3]);
    let now = Instant::now();
    let mut cache = crate::pull_request::Cache::default();
    let mut pr = crate::pull_request::fixture().unwrap();
    pr.additions = 1730;
    pr.deletions = 31;
    cache.seed(input("worktree/sidebar-child"), pr, now);
    let mut git = Git::default();
    let counted = |additions, deletions| Counted {
        lines: Status {
            additions,
            deletions,
            untracked: 0,
            ..Default::default()
        },
        dirty: true,
    };
    git.seed_counts(input("worktree/sidebar-child"), counted(42, 7), now);
    // Untracked or binary changes alone have no lines to count: the dirty
    // mark shows them, and no `+0/-0` is drawn.
    git.seed_counts(input("develop"), counted(0, 0), now);

    let pull_request = Some(("+1730".to_owned(), "-31".to_owned()));
    let uncommitted = Some(("+42".to_owned(), "-7".to_owned()));
    assert_eq!(
        counts(child, &cache, &git, ChangeCounts::PullRequest),
        pull_request
    );
    assert_eq!(
        counts(child, &cache, &git, ChangeCounts::Local),
        uncommitted
    );
    assert_eq!(counts(head, &cache, &git, ChangeCounts::PullRequest), None);
    assert_eq!(counts(head, &cache, &git, ChangeCounts::Local), None);
    let marks = crate::teleport::Marks::detached();
    let badge = |workspace, git| {
        workspace_badge(
            workspace,
            None,
            (git, ChangeCounts::Local),
            (&marks, crate::endpoint::LOCAL),
            (None, false),
            &Theme::default(),
        )
    };
    assert!(badge(head, Some(&git)).unwrap().dirty);
    // A row on another device never reads this machine's Git answers, even
    // where its path and branch match a local checkout.
    assert!(badge(child, None).is_none());
    assert!(badge(head, None).is_none());
    // A checkout whose probe only knows it is dirty shows no local counts,
    // never the pull request's in their place.
    let mut git = Git::default();
    git.seed_probe(input("worktree/sidebar-child"), true, now);
    assert_eq!(counts(child, &cache, &git, ChangeCounts::Local), None);
}

/// The row's pull request slot as drawn, or `None` when it is empty.
fn pr_slot(
    workspace: &ClientShellWorkspace,
    cache: &crate::pull_request::Cache,
    git: &Git,
) -> Option<String> {
    let marks = crate::teleport::Marks::detached();
    workspace_badge(
        workspace,
        Some((cache, &crate::pull_request::View::default())),
        (Some(git), ChangeCounts::PullRequest),
        (&marks, crate::endpoint::LOCAL),
        (None, false),
        &Theme::default(),
    )?
    .pr
    .map(|pr| pr.number)
}

fn ahead(head: &str) -> Ahead {
    Ahead {
        commits: 2,
        head: head.into(),
    }
}

#[test]
fn commits_without_a_pull_request_take_the_number_slot_once_a_lookup_finished() {
    let snapshot = layout_tests::snapshot(6);
    let child = &snapshot.workspaces[4];
    let branch = input("worktree/sidebar-child");
    let now = Instant::now();
    let mut cache = crate::pull_request::Cache::default();
    let mut git = Git::default();
    git.seed_ahead(branch.clone(), ahead("tip"), now);
    assert_eq!(pr_slot(child, &cache, &git), None, "not looked up yet");
    cache.seed_none(branch.clone(), now);
    assert_eq!(pr_slot(child, &cache, &git).as_deref(), Some("+PR"));
    // Nothing ahead, as on a branch just cut or the default branch.
    assert_eq!(pr_slot(child, &cache, &Git::default()), None);
}

#[test]
fn a_merged_pull_request_keeps_its_number_until_the_branch_moves_past_it() {
    let snapshot = layout_tests::snapshot(6);
    let child = &snapshot.workspaces[4];
    let branch = input("worktree/sidebar-child");
    let now = Instant::now();
    let mut pr = crate::pull_request::fixture().unwrap();
    pr.state = crate::pull_request::State::Merged;
    pr.head_ref_oid = "merged".into();
    let number = format!("#{}", pr.number);
    let mut cache = crate::pull_request::Cache::default();
    cache.seed(branch.clone(), pr, now);
    let mut git = Git::default();
    git.seed_ahead(branch.clone(), ahead("merged"), now);
    assert_eq!(pr_slot(child, &cache, &git), Some(number));
    git.seed_ahead(branch, ahead("newer"), now);
    assert_eq!(pr_slot(child, &cache, &git).as_deref(), Some("+PR"));
    assert_eq!(
        counts(child, &cache, &git, ChangeCounts::PullRequest),
        None,
        "the merged pull request's counts do not describe the new commits"
    );
}
