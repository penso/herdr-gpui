//! Review in the "…" menu and the titlebar's Git menu follow any repository
//! the focused space is in, not only a worktree Herdr made, and still show
//! nothing for a folder outside one.
use super::*;
use crate::{menu::Page, pull_request::Input};
use herdr_client::protocol::ClientShellWorktree;

/// Shows workspace `w0` in `directory` as the window's own local daemon
/// reports it, and lets the Git chip follow it as the poll task does.
fn space_in(
    view: &Entity<HerdrWindow>,
    cx: &mut VisualTestContext,
    directory: &str,
    branch: Option<&str>,
    worktree: Option<ClientShellWorktree>,
) {
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            let mut shown = (**view.live.snapshot.as_ref().unwrap()).clone();
            let space = shown
                .workspaces
                .iter_mut()
                .find(|workspace| workspace.workspace_id == "w0")
                .unwrap();
            space.new_workspace_cwd = directory.into();
            space.branch = branch.map(Into::into);
            space.worktree = worktree;
            view.live.snapshot = Some(Arc::new(shown));
            view.live.status = crate::state::ConnectionStatus::Connected;
            view.live.local_daemon_peer = true;
            view.update_git();
        })
    });
}

fn open_git_menu(view: &Entity<HerdrWindow>, cx: &mut VisualTestContext) -> Option<Page> {
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.dismiss_menu(window, cx);
            view.open_git_menu(Point::default(), window, cx);
            view.menu.page
        })
    })
}

#[gpui::test]
fn a_space_in_a_cloned_repository_offers_review_and_the_git_menu(cx: &mut TestAppContext) {
    let (view, cx) = window(cx);
    let group = groups(&view, cx)[0];
    // A clone opened as a space, in a subdirectory, with no worktree made
    // through Herdr: the daemon reports its branch but no worktree.
    let directory = std::env::temp_dir().join("clone/src");
    let directory = directory.to_str().unwrap();
    space_in(&view, cx, directory, Some("main"), None);
    let input = Input {
        checkout: Some(directory.into()),
        repo_key: None,
        branch: "main".into(),
    };
    view.read_with(cx, |view, _| assert_eq!(view.git.tracked(), Some(&input)));
    assert!(actions(&view, cx, group).contains(&Action::Review));
    assert_eq!(open_git_menu(&view, cx), Some(Page::Git));
    // Its pull request, found by the same directory, names the Git menu's
    // pull request row rather than offering to create another.
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            let pr = crate::pull_request::fixture().unwrap();
            view.seed_pr(input, pr, std::time::Instant::now());
            assert_eq!(view.git_pull_request().map(|pr| pr.number), Some(8));
        })
    });
}

#[gpui::test]
fn a_worktree_herdr_made_still_offers_review_by_its_daemon_key(cx: &mut TestAppContext) {
    let (view, cx) = window(cx);
    let group = groups(&view, cx)[0];
    let key = std::env::temp_dir().join("repo/.git");
    let key = key.to_str().unwrap();
    let worktree = ClientShellWorktree {
        key: key.into(),
        label: "repo".into(),
        is_linked_worktree: true,
    };
    let directory = std::env::temp_dir().join("elsewhere");
    space_in(
        &view,
        cx,
        directory.to_str().unwrap(),
        Some("feature"),
        Some(worktree),
    );
    view.read_with(cx, |view, _| {
        assert_eq!(
            view.git.tracked(),
            Some(&Input {
                checkout: None,
                repo_key: Some(key.into()),
                branch: "feature".into(),
            })
        );
    });
    assert!(actions(&view, cx, group).contains(&Action::Review));
    assert_eq!(open_git_menu(&view, cx), Some(Page::Git));
}

#[gpui::test]
fn a_folder_outside_any_repository_offers_neither(cx: &mut TestAppContext) {
    let (view, cx) = window(cx);
    let group = groups(&view, cx)[0];
    let directory = std::env::temp_dir().join("notes");
    // The daemon reports no branch for a folder Git does not know.
    space_in(&view, cx, directory.to_str().unwrap(), None, None);
    view.read_with(cx, |view, _| assert_eq!(view.git.tracked(), None));
    assert!(!actions(&view, cx, group).contains(&Action::Review));
    assert_eq!(open_git_menu(&view, cx), None);
}
