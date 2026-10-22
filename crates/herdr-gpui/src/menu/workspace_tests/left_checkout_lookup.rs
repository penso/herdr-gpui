//! Repository lookups (the pull request section, the new worktree dialog's
//! branch and GitHub tabs) for a main checkout whose shell left the
//! repository Herdr keeps for it, and for a linked checkout routed through
//! such a main checkout.

use super::*;
use crate::pull_request::{Input, Origin};

const MOVED: &str = "/fixture/wgsextract-cli";

fn open(view: &mut HerdrWindow, workspace: usize, linked_route: bool) {
    view.live.status = crate::state::ConnectionStatus::Connected;
    let snapshot = std::sync::Arc::make_mut(view.live.snapshot.as_mut().unwrap());
    snapshot.workspaces = sidebar::layout_tests::snapshot(7).workspaces;
    snapshot.workspaces[3].new_workspace_cwd = MOVED.into();
    snapshot.workspaces[3].branch = Some("main".into());
    let target = if linked_route {
        WorkspaceTarget::for_new_worktree(snapshot, &snapshot.workspaces[workspace]).unwrap()
    } else {
        WorkspaceTarget::new(snapshot, &snapshot.workspaces[workspace])
    };
    view.menu.target = Some(target);
}

fn move_again(view: &mut HerdrWindow) {
    let snapshot = std::sync::Arc::make_mut(view.live.snapshot.as_mut().unwrap());
    snapshot.workspaces[3].new_workspace_cwd = "/fixture/third".into();
}

/// A moved main checkout is looked up by the directory it is in, and stops
/// being the menu's workspace once it moves again, rather than keeping the
/// previous repository's pull request.
#[gpui::test]
#[cfg(unix)]
fn a_moved_main_checkout_is_looked_up_where_it_is_until_it_moves_again(
    cx: &mut gpui::TestAppContext,
) {
    let (view, cx) = cx.add_window_view(sidebar::layout_tests::fixture_window);
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            open(view, 3, false);
            assert_eq!(
                view.menu_target_input(&Origin::Local).unwrap(),
                Input {
                    checkout: Some(MOVED.into()),
                    repo_key: None,
                    branch: "main".into(),
                }
            );
            move_again(view);
            assert!(view.menu_target_workspace().is_none());
            assert!(matches!(
                view.menu_target_input(&Origin::Local),
                Err(crate::Error::StaleWorkspace)
            ));
        })
    });
}

/// A linked checkout routed through a moved main checkout is looked up by
/// its repository's Git directory and its own branch, never the moved main
/// checkout's branch of another repository. A move of the main checkout to
/// yet another directory leaves that unchanged.
#[gpui::test]
fn a_linked_route_reads_its_repository_with_its_own_branch(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(sidebar::layout_tests::fixture_window);
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            open(view, 4, true);
            let expected = Input {
                checkout: None,
                repo_key: Some(sidebar::layout_tests::REPO_KEY.into()),
                branch: "worktree/sidebar-child".into(),
            };
            assert_eq!(view.menu_target_input(&Origin::Local).unwrap(), expected);
            move_again(view);
            assert_eq!(view.menu_target_input(&Origin::Local).unwrap(), expected);
            // Once the main checkout is back in its repository, the route no
            // longer stands.
            let snapshot = std::sync::Arc::make_mut(view.live.snapshot.as_mut().unwrap());
            snapshot.workspaces[3].new_workspace_cwd = sidebar::layout_tests::REPO_KEY
                .trim_end_matches("/.git")
                .into();
            assert!(view.menu_target_workspace().is_none());
        })
    });
}
