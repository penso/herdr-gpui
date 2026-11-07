//! The focused branch's commits that no pull request carries yet.
use super::{Page, REPO_KEY, Row};
use crate::{
    git::{Ahead, Git, Status},
    pull_request::{Input, State},
};
use gpui::{TestAppContext, point, px};
use std::time::Instant;

fn input() -> Input {
    Input {
        checkout: None,
        repo_key: Some(REPO_KEY.into()),
        branch: "develop".into(),
    }
}

fn ahead_git() -> Git {
    let mut git = Git::fixture(input(), Status::default());
    let ahead = Ahead {
        commits: 3,
        head: "tip".into(),
    };
    git.seed_ahead(input(), ahead, Instant::now());
    git
}

#[gpui::test]
fn the_count_waits_for_a_finished_lookup_and_an_open_pull_request_ends_it(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(crate::sidebar::layout_tests::fixture_window);
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            let now = Instant::now();
            view.git = ahead_git();
            assert_eq!(view.git_unopened_commits(), None, "signed out");
            view.menu.github = crate::github::Auth::connected_fixture();
            view.live.local_daemon_peer = true;
            view.sync_pr_scope();
            assert_eq!(view.git_unopened_commits(), None, "not looked up yet");
            view.seed_no_pr(input(), now);
            assert_eq!(view.git_unopened_commits(), Some(3));
            let mut pr = crate::pull_request::fixture().unwrap();
            pr.state = State::Open;
            view.seed_pr(input(), pr, now);
            assert_eq!(view.git_unopened_commits(), None);
        })
    });
}

#[gpui::test]
fn the_titlebar_note_opens_the_menu_on_its_row_without_creating_anything(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(crate::sidebar::layout_tests::fixture_window);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.git = ahead_git();
            view.open_git_menu_on_pull_request(point(px(10.), px(10.)), window, cx);
            assert_eq!(view.menu.page, Some(Page::Git));
            assert_eq!(view.menu.git_selected, Some(Row::PullRequest));
            assert!(view.git.running().is_none());
        })
    });
}
