use super::*;
use crate::{
    config::ChangeCounts,
    git::{Counted, Git, Status},
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
