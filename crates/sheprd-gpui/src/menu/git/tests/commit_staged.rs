//! With something staged, the commit dialog takes only that unless told
//! otherwise, so hunks staged from a review are not swept up by `git add -A`.
use super::*;
use crate::git::{Action, Git};

fn input() -> crate::pull_request::Input {
    crate::pull_request::Input {
        checkout: None,
        repo_key: Some(REPO_KEY.into()),
        branch: "develop".into(),
    }
}

fn staged(files: u64) -> Status {
    Status {
        staged: files,
        ..status(3, 1, 2)
    }
}

/// Opens the commit dialog over a checkout with `status`, a message typed.
fn commit_dialog(
    status: Status,
    cx: &mut TestAppContext,
) -> (gpui::Entity<crate::SheprdWindow>, &mut VisualTestContext) {
    let (view, cx) = cx.add_window_view(crate::sidebar::layout_tests::fixture_window);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.git = Git::fixture(input(), status);
            view.open_git_menu(point(px(900.), px(20.)), window, cx);
            view.activate_git_row(Row::Commit, window, cx);
            view.menu.input.as_mut().unwrap().text = "A message".into();
        })
    });
    (view, cx)
}

fn draw(cx: &mut VisualTestContext) {
    cx.update(|window, cx| crate::sidebar::layout_tests::full_draw(window, cx).clear(cx));
}

#[gpui::test]
fn something_staged_commits_alone_by_default(cx: &mut TestAppContext) {
    let (view, cx) = commit_dialog(staged(2), cx);
    draw(cx);
    assert!(cx.debug_bounds("git-commit-staged").is_some());
    cx.update(|_, cx| view.update(cx, |view, cx| view.submit_git_commit(cx)));
    view.read_with(cx, |view, _| {
        assert_eq!(
            view.git.running(),
            Some(&Action::CommitStaged("A message".into()))
        );
    });
}

#[gpui::test]
fn the_choice_can_stage_everything_after_all(cx: &mut TestAppContext) {
    let (view, cx) = commit_dialog(staged(1), cx);
    draw(cx);
    let choice = cx.debug_bounds("git-commit-staged").unwrap().center();
    cx.simulate_click(choice, gpui::Modifiers::none());
    view.read_with(cx, |view, _| assert!(!view.git_commits_staged()));
    cx.update(|_, cx| view.update(cx, |view, cx| view.submit_git_commit(cx)));
    view.read_with(cx, |view, _| {
        assert_eq!(
            view.git.running(),
            Some(&Action::Commit("A message".into()))
        );
    });
}

#[gpui::test]
fn a_status_landing_after_the_dialog_opened_keeps_the_commit_to_what_is_staged(
    cx: &mut TestAppContext,
) {
    let (view, cx) = commit_dialog(status(3, 1, 2), cx);
    view.read_with(cx, |view, _| assert!(!view.git_commits_staged()));
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.git = Git::fixture(input(), staged(1));
            view.submit_git_commit(cx);
        })
    });
    view.read_with(cx, |view, _| {
        assert_eq!(
            view.git.running(),
            Some(&Action::CommitStaged("A message".into()))
        );
    });
}

#[gpui::test]
fn the_users_choice_outlives_a_new_status(cx: &mut TestAppContext) {
    let (view, cx) = commit_dialog(staged(1), cx);
    draw(cx);
    let choice = cx.debug_bounds("git-commit-staged").unwrap().center();
    cx.simulate_click(choice, gpui::Modifiers::none());
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.git = Git::fixture(input(), staged(2));
            view.submit_git_commit(cx);
        })
    });
    view.read_with(cx, |view, _| {
        assert_eq!(
            view.git.running(),
            Some(&Action::Commit("A message".into()))
        );
    });
}

#[gpui::test]
fn nothing_staged_offers_no_choice_and_stages_everything(cx: &mut TestAppContext) {
    let (view, cx) = commit_dialog(staged(0), cx);
    draw(cx);
    assert!(cx.debug_bounds("git-commit-staged").is_none());
    cx.update(|_, cx| view.update(cx, |view, cx| view.submit_git_commit(cx)));
    view.read_with(cx, |view, _| {
        assert_eq!(
            view.git.running(),
            Some(&Action::Commit("A message".into()))
        );
    });
}
