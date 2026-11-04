use super::*;

#[gpui::test]
fn the_main_account_offers_the_github_cli_and_a_host_does_not(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(crate::sidebar::layout_tests::fixture_window);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.github_fixture(false, window, cx);
            assert!(!view.github_actions().contains(&Action::GitHubCli));
            view.menu.github = crate::github::Auth::github_cli_fixture(false);
            assert_eq!(
                view.github_actions(),
                [
                    Action::GitHubCli,
                    Action::Start,
                    Action::Token,
                    Action::Close
                ]
            );
            cx.notify();
        });
        crate::sidebar::layout_tests::full_draw(window, cx).clear(cx);
    });
    assert!(cx.debug_bounds("github-cli").is_some());
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            // This computer's CLI is not a saved host's own account.
            let account = crate::github::Account::host(&HOST["ssh:".len()..]).unwrap();
            select_host(view, crate::github::Auth::for_account(account));
            assert!(!view.github_actions().contains(&Action::GitHubCli));
            cx.notify();
        });
        crate::sidebar::layout_tests::full_draw(window, cx).clear(cx);
    });
    assert!(cx.debug_bounds("github-cli").is_none());
}

#[gpui::test]
fn a_github_cli_session_says_so_and_how_to_reach_private_repositories(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(crate::sidebar::layout_tests::fixture_window);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.github_fixture(false, window, cx);
            view.menu.github = crate::github::Auth::github_cli_fixture(true);
            assert_eq!(
                view.github_actions(),
                [Action::SignOut, Action::Add, Action::Close]
            );
            cx.notify();
        });
        crate::sidebar::layout_tests::full_draw(window, cx).clear(cx);
    });
    assert!(cx.debug_bounds("github-connected").is_some());
    assert!(cx.debug_bounds("github-cli-note").is_some());
    assert!(cx.debug_bounds("github-cli").is_none());
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.menu.github = crate::github::Auth::connected_fixture();
            cx.notify();
        });
        crate::sidebar::layout_tests::full_draw(window, cx).clear(cx);
    });
    assert!(cx.debug_bounds("github-cli-note").is_none());
}
