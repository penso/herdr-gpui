use super::*;
use crate::{
    config::GitHubAccount,
    github::{Account, Auth, Profile},
    menu::github::AdditionalAccount,
};
use secrecy::ExposeSecret;
use std::sync::Arc;

const ID: &str = "0123456789abcdef0123456789abcdef";

/// Lists one additional account in the config and gives the window its
/// sign-in, signed in as `login` when given.
fn add_account(view: &mut crate::SheprdWindow, login: Option<&str>) {
    view.config.github.accounts = vec![GitHubAccount {
        id: ID.into(),
        owners: vec!["listed-owner".into()],
    }];
    let mut auth = Auth::for_account(Account::additional(ID).unwrap());
    auth.profile =
        login.map(|login| Profile::fixture(login, Arc::new("emu-token".into()), &["emu-org"]));
    view.menu.github_accounts = vec![AdditionalAccount {
        id: ID.into(),
        owners: vec!["listed-owner".into()],
        auth,
    }];
}

#[gpui::test]
fn repositories_an_additional_account_owns_are_read_with_it(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(crate::sidebar::layout_tests::fixture_window);
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            view.menu.github = Auth::connected_fixture();
            add_account(view, None);
            // Not signed in yet: everything stays with the main account.
            let tokens = view.pr_tokens().unwrap();
            assert_eq!(tokens.key(), "fixture-user");
            assert_eq!(
                tokens.for_owner("fabien_microsoft").expose_secret(),
                "fixture-token"
            );

            add_account(view, Some("fabien_microsoft"));
            let tokens = view.pr_tokens().unwrap();
            assert_eq!(
                tokens.key(),
                "fixture-user,fabien_microsoft(fabien_microsoft emu-org listed-owner)"
            );
            for owner in ["fabien_microsoft", "emu-org", "listed-owner"] {
                assert_eq!(tokens.for_owner(owner).expose_secret(), "emu-token");
            }
            assert_eq!(
                tokens.for_owner("rust-lang").expose_secret(),
                "fixture-token"
            );
            assert_eq!(
                view.menu.github_accounts[0].covered(),
                ["fabien_microsoft", "emu-org", "listed-owner"]
            );
            // Local-only work reads with the same accounts.
            assert!(view.main_tokens().unwrap().same(&tokens));

            // Without a main sign-in nothing is read, as before.
            view.menu.github = Auth::default();
            assert!(view.pr_tokens().is_none());
        });
    });
}

#[gpui::test]
fn the_page_switches_to_an_additional_account_and_back(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(crate::sidebar::layout_tests::fixture_window);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.github_fixture(false, window, cx);
            view.menu.github = Auth::connected_fixture();
            assert!(view.github_actions().contains(&Action::Add));
            add_account(view, None);
            cx.notify();
        });
        crate::sidebar::layout_tests::full_draw(window, cx).clear(cx);
    });
    assert!(cx.debug_bounds("github-accounts").is_some());
    assert!(cx.debug_bounds("github-account-note").is_none());
    cx.simulate_keystrokes("right");
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            assert_eq!(view.menu.github_account.as_deref(), Some(ID));
            assert!(!view.github_auth().connected());
            // Its own sign-in, a pasted token, or removal; never the local CLI.
            assert_eq!(
                view.github_actions(),
                [Action::Start, Action::Token, Action::SignOut, Action::Close]
            );
            cx.notify();
        });
        crate::sidebar::layout_tests::full_draw(window, cx).clear(cx);
    });
    assert!(cx.debug_bounds("github-account-note").is_some());
    cx.simulate_keystrokes("right");
    cx.update(|_, cx| {
        assert!(view.read(cx).menu.github_account.is_none());
        assert!(view.read(cx).github_auth().connected());
    });
}

#[gpui::test]
fn the_token_field_keeps_typing_and_escape_closes_only_it(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(crate::sidebar::layout_tests::fixture_window);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.github_fixture(false, window, cx);
            view.menu.github = Auth::connected_fixture();
            add_account(view, None);
            view.select_github_account(Some(ID.into()));
            cx.notify();
        });
        crate::sidebar::layout_tests::full_draw(window, cx).clear(cx);
    });
    cx.simulate_keystrokes("t");
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            assert!(view.menu.github_token.is_some());
            assert_eq!(
                view.github_actions(),
                [Action::SubmitToken, Action::CancelToken, Action::Close]
            );
            cx.notify();
        });
        crate::sidebar::layout_tests::full_draw(window, cx).clear(cx);
    });
    assert!(cx.debug_bounds("github-token").is_some());
    // Letters that are page shortcuts elsewhere are typed into the field.
    cx.simulate_keystrokes("d a t");
    cx.update(|_, cx| {
        let view = view.read(cx);
        let input = view.menu.github_token.as_ref().unwrap();
        assert_eq!(input.read(cx).text(), "dat");
        assert_eq!(view.menu.github_accounts.len(), 1);
    });
    cx.simulate_keystrokes("escape");
    cx.update(|_, cx| {
        let view = view.read(cx);
        assert!(view.menu.github_token.is_none());
        assert_eq!(view.menu.page, Some(crate::menu::Page::GitHub));
        assert_eq!(view.menu.github_account.as_deref(), Some(ID));
    });
}

#[gpui::test]
fn an_account_gone_from_the_config_is_dropped_and_deselected(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(crate::sidebar::layout_tests::fixture_window);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.github_fixture(false, window, cx);
            add_account(view, Some("fabien_microsoft"));
            view.select_github_account(Some(ID.into()));
            view.config.github.accounts[0].owners = vec!["renamed".into()];
            view.poll_github(window, cx);
            // Owners follow the config without signing the account out.
            assert_eq!(view.menu.github_accounts[0].owners, ["renamed"]);
            assert!(view.github_auth().connected());

            view.config.github.accounts.clear();
            view.poll_github(window, cx);
            assert!(view.menu.github_accounts.is_empty());
            assert!(view.menu.github_account.is_none());
        });
    });
}

#[gpui::test]
fn an_added_account_is_shown_once_its_sign_in_exists(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(crate::sidebar::layout_tests::fixture_window);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.github_fixture(false, window, cx);
            view.menu.github = Auth::connected_fixture();
            // The config write has not landed yet: the device's account stays.
            view.menu.github_adding = Some(ID.into());
            view.poll_github(window, cx);
            assert!(view.menu.github_account.is_none());
            assert!(view.github_auth().connected());

            add_account(view, None);
            view.poll_github(window, cx);
            assert_eq!(view.menu.github_account.as_deref(), Some(ID));
            assert!(view.menu.github_adding.is_none());
            assert!(!view.github_auth().connected());
        });
    });
}
