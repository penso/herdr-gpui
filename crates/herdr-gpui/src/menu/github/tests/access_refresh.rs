use super::*;
use crate::{
    config::GitHubAccount,
    github::{Access, Auth},
    menu::github::AdditionalAccount,
};

const ID: &str = "0123456789abcdef0123456789abcdef";

fn waiting() -> Auth {
    let mut auth = Auth::connected_fixture();
    auth.profile.as_mut().unwrap().access = Access::NotInstalled;
    auth
}

fn add_account(view: &mut crate::HerdrWindow) {
    view.config.github.accounts = vec![GitHubAccount {
        id: ID.into(),
        owners: Vec::new(),
    }];
    view.menu.github_accounts = vec![AdditionalAccount {
        id: ID.into(),
        owners: Vec::new(),
        auth: waiting(),
    }];
}

#[gpui::test]
fn the_page_rechecks_all_shown_waiting_accounts_only_while_open(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(crate::sidebar::layout_tests::fixture_window);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.menu.github = waiting();
            add_account(view);
            view.menu.page = None;
            view.poll_github(window, cx);
            assert!(!view.menu.github.access_recheck_scheduled_fixture());
            assert!(
                !view.menu.github_accounts[0]
                    .auth
                    .access_recheck_scheduled_fixture()
            );
            view.menu.page = Some(crate::menu::Page::GitHub);
            view.poll_github(window, cx);
            assert!(view.menu.github.access_recheck_scheduled_fixture());
            assert!(
                view.menu.github_accounts[0]
                    .auth
                    .access_recheck_scheduled_fixture()
            );
        });
    });
}

#[gpui::test]
fn a_remote_device_rechecks_its_own_account_and_the_additional_accounts(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(crate::sidebar::layout_tests::fixture_window);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.menu.github = waiting();
            select_host(view, waiting());
            add_account(view);
            view.menu.page = Some(crate::menu::Page::GitHub);
            view.poll_github(window, cx);
            assert!(view.menu.github_hosts[HOST].access_recheck_scheduled_fixture());
            assert!(
                view.menu.github_accounts[0]
                    .auth
                    .access_recheck_scheduled_fixture()
            );
            assert!(!view.menu.github.access_recheck_scheduled_fixture());
        });
    });
}

#[gpui::test]
fn switching_accounts_and_opening_approval_schedule_the_target_account(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(crate::sidebar::layout_tests::fixture_window);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.menu.github = waiting();
            add_account(view);
            view.select_github_account(Some(ID.into()));
            assert!(
                view.menu.github_accounts[0]
                    .auth
                    .access_recheck_scheduled_fixture()
            );
            assert!(!view.menu.github.access_recheck_scheduled_fixture());
            view.menu.github_accounts[0].auth = waiting();
            view.open_github_attention(cx);
            assert!(
                view.menu.github_accounts[0]
                    .auth
                    .access_recheck_scheduled_fixture()
            );
            assert!(!view.menu.github.access_recheck_scheduled_fixture());
        });
    });
}

#[gpui::test]
fn a_refresh_updates_its_own_routes_after_switching_tabs(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(crate::sidebar::layout_tests::fixture_window);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.menu.github = Auth::connected_fixture();
            add_account(view);
            view.menu.page = Some(crate::menu::Page::GitHub);
            view.select_github_account(Some(ID.into()));
            let before = view.pr_tokens().unwrap();
            let mut profile = crate::github::Profile::fixture(
                "fixture-user",
                std::sync::Arc::new("fixture-token".into()),
                &["private-org"],
            );
            profile.access = Access::Ready;
            view.menu.github_accounts[0]
                .auth
                .complete_profile_fixture(Ok(Some(profile)));
            view.select_github_account(None);
            view.poll_github(window, cx);
            assert!(view.menu.github_account.is_none());
            assert_eq!(view.menu.page, Some(crate::menu::Page::GitHub));
            assert!(
                view.menu
                    .github
                    .profile
                    .as_ref()
                    .unwrap()
                    .organizations
                    .is_none()
            );
            let after = view.pr_tokens().unwrap();
            assert!(
                !after.same(&before),
                "changed owners must fence cached PR results even with the same token"
            );
            assert_ne!(after.key(), before.key());
            view.select_github_account(Some(ID.into()));
            assert!(view.github_account_note().unwrap().contains("private-org"));
            assert!(view.github_attention().is_none());
        });
    });
}
