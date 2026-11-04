//! A GitLab project in the new worktree dialog: its issues list without a
//! GitHub account and create a checkout like a GitHub issue; it lists no
//! pull requests.
use super::*;
use crate::{menu::worktree_source::faded_hint, repo_items::Forge};

fn gitlab() -> Origin {
    Origin {
        owner: "team".into(),
        repo: "app".into(),
        forge: Forge::GitLab {
            host: "gitlab.example.com".into(),
        },
        ..Origin::default()
    }
}

fn issue() -> Item {
    Item {
        kind: Kind::Issue,
        number: 7,
        title: "Add a flag".into(),
        url: "https://gitlab.example.com/team/app/-/issues/7".into(),
        author: "ana".into(),
        head: None,
        fork_owner: None,
        remote: Remote::Origin,
        draft: false,
    }
}

#[gpui::test]
fn a_gitlab_projects_issues_need_no_github_account(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(sidebar::layout_tests::fixture_window);
    cx.simulate_resize(gpui::size(gpui::px(900.), gpui::px(700.)));
    open_dialog(&view, cx, false, Tab::New);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            let source = view.menu.worktree.as_mut().unwrap();
            source.install(gitlab(), vec![issue()]);
            view.select_worktree_tab(Tab::Items(Kind::Issue), window, cx);
        });
    });
    draw(cx);
    assert_eq!(tab(&view, cx), Tab::Items(Kind::Issue));
    assert!(cx.debug_bounds("worktree-row-0").is_some());
    // Merge requests are not listed, so the pull request tab stays shut.
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.select_worktree_tab(Tab::Items(Kind::PullRequest), window, cx);
        });
    });
    assert_eq!(tab(&view, cx), Tab::Items(Kind::Issue));
    assert!(faded_hint(Kind::PullRequest, true).contains("merge requests"));

    // Picking the issue asks for a checkout on a new branch named after it,
    // as a GitHub issue does.
    cx.update(|_, cx| {
        let view = view.read(cx);
        let target = view.menu.target.as_ref().unwrap();
        let snapshot = view.live.snapshot.as_ref().unwrap();
        let (method, params) = item_request(target, snapshot, &issue()).unwrap();
        assert_eq!(method, sheprd_client::Method::WorktreeCreate);
        assert_eq!(params["branch"], "7-add-a-flag");
        assert_eq!(params["base"], "HEAD");
        assert_eq!(params["label"], "#7 Add a flag");
    });
}

/// With a GitHub account, a GitLab project still lists no pull requests.
#[gpui::test]
fn a_gitlab_project_lists_no_pull_requests_even_signed_in(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(sidebar::layout_tests::fixture_window);
    cx.simulate_resize(gpui::size(gpui::px(900.), gpui::px(700.)));
    open_dialog(&view, cx, true, Tab::New);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            let source = view.menu.worktree.as_mut().unwrap();
            source.install(gitlab(), vec![issue()]);
            view.select_worktree_tab(Tab::Items(Kind::PullRequest), window, cx);
            assert_eq!(view.menu.worktree.as_ref().unwrap().tab, Tab::New);
            view.select_worktree_tab(Tab::Items(Kind::Issue), window, cx);
            assert_eq!(
                view.menu.worktree.as_ref().unwrap().tab,
                Tab::Items(Kind::Issue)
            );
        });
    });
}

/// A pull request tab opened before the listing found a GitLab remote can
/// no longer be used once it has; Ctrl-Tab still moves from it, either way.
#[gpui::test]
fn ctrl_tab_leaves_a_pull_request_tab_gitlab_does_not_have(cx: &mut gpui::TestAppContext) {
    for (keys, landed) in [
        ("ctrl-tab", Tab::Items(Kind::Issue)),
        ("ctrl-shift-tab", Tab::Branches),
    ] {
        let (view, cx) = cx.add_window_view(sidebar::layout_tests::fixture_window);
        cx.simulate_resize(gpui::size(gpui::px(900.), gpui::px(700.)));
        open_dialog(&view, cx, true, Tab::Items(Kind::PullRequest));
        view.update(cx, |view, _| {
            let source = view.menu.worktree.as_mut().unwrap();
            source.install(gitlab(), vec![issue()]);
        });
        assert_eq!(tab(&view, cx), Tab::Items(Kind::PullRequest));
        cx.simulate_keystrokes(keys);
        assert_eq!(tab(&view, cx), landed, "{keys}");
    }
}
