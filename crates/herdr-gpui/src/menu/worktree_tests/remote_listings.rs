use super::*;
use crate::pull_request::Origin as Host;

fn select_remote(view: &mut HerdrWindow) {
    view.endpoints.push(crate::endpoint::Endpoint::new(
        "ssh:work".into(),
        "Work".into(),
        herdr_client::ConnectTarget::Ssh {
            target: "me@work".into(),
            session: "default".into(),
        },
        true,
    ));
    view.selected_endpoint = 1;
    view.live.local_daemon_peer = false;
}

#[gpui::test]
fn remote_picker_uses_the_hosts_account_and_repository(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(sidebar::layout_tests::fixture_window);
    cx.update(|_, cx| view.update(cx, |view, _| select_remote(view)));
    open_dialog(&view, cx, true, Tab::New);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.live.local_daemon_peer = false;
            let main = view.repo_items_request().unwrap();
            assert_eq!(main.0.host, Host::Ssh("me@work".into()));
            assert_eq!(
                main.0.input.repo_key.as_deref(),
                Some("/fixture/agent-launcher/.git")
            );
            assert_eq!(main.1.key(), view.main_tokens().unwrap().key());

            let mut auth = crate::github::Auth::connected_fixture();
            auth.profile.as_mut().unwrap().login = "work-user".into();
            view.menu.github_hosts.insert("ssh:work".into(), auth);
            view.menu.github = Default::default();
            let (_, tokens) = view.repo_items_request().unwrap();
            assert_eq!(tokens.key(), view.pr_tokens().unwrap().key());
            assert_ne!(tokens.key(), main.1.key());

            // A device-only sign-in enables both tabs even with Local signed out.
            for kind in [Kind::Issue, Kind::PullRequest] {
                view.select_worktree_tab(Tab::Items(kind), window, cx);
                let source = view.menu.worktree.as_ref().unwrap();
                assert_eq!(source.tab, Tab::Items(kind));
                assert!(source.lookup.message.is_none());
                assert!(source.item(0).is_some());
            }
            view.create_from_repo_item(1, cx);
            let source = view.menu.worktree.as_ref().unwrap();
            assert!(source.lookup.loading);
            assert!(matches!(&source.pending, Some(Pending::Item(item)) if item.number == 51));
            assert!(view.menu.error.is_none());
        });
    });
}

#[gpui::test]
fn remote_picker_rejects_stale_disconnected_and_unknown_endpoints(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(sidebar::layout_tests::fixture_window);
    cx.update(|_, cx| view.update(cx, |view, _| select_remote(view)));
    open_dialog(&view, cx, true, Tab::New);
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            view.live.local_daemon_peer = false;
            view.endpoints[1].generation += 1;
            assert!(matches!(
                view.repo_items_request(),
                Err(crate::Error::StaleWorkspace)
            ));
            view.endpoints[1].generation -= 1;
            view.live.status = crate::state::ConnectionStatus::Disconnected;
            assert!(view.repo_items_request().is_err());
            view.live.status = crate::state::ConnectionStatus::Connected;
            view.endpoints[1].connection.target =
                herdr_client::ConnectTarget::Socket("/forwarded.sock".into());
            assert!(matches!(
                view.repo_items_request(),
                Err(crate::Error::GitHub(herdr_github::Error::PullRequest(
                    herdr_github::pull_request::Error::UntrustedEndpoint
                )))
            ));
        });
    });
}
