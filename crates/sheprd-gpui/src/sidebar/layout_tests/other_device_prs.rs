use super::*;

fn input(branch: &str) -> crate::pull_request::Input {
    crate::pull_request::Input {
        checkout: None,
        repo_key: Some(REPO_KEY.into()),
        branch: branch.into(),
    }
}

/// With a remote device selected, signed in to its own GitHub account, the
/// local device's rows still wear the pull requests found while it was
/// selected, and the remote's row with the same repository and branch does
/// not borrow them.
#[gpui::test]
fn another_devices_rows_keep_their_pull_requests(cx: &mut gpui::TestAppContext) {
    let (_fixture, cx) = cx.add_window_view(|window, cx| {
        let view = cx.new(|cx| {
            let mut view = fixture_window(window, cx);
            let now = std::time::Instant::now();
            view.seed_pr(
                input("develop"),
                crate::pull_request::fixture().unwrap(),
                now,
            );
            // The unselected local device draws what it kept when it was left.
            view.endpoints[0].live = view.live.clone();
            view.live.local_daemon_peer = false;
            let remote = Arc::make_mut(view.live.snapshot.as_mut().unwrap());
            // The group head: same key and branch as the local one.
            remote.workspaces[3].label = "remote launcher".into();
            view.endpoints.push(crate::endpoint::Endpoint::new(
                "ssh:test".into(),
                "Remote".into(),
                ConnectTarget::Ssh {
                    target: "me@host".into(),
                    session: "default".into(),
                },
                true,
            ));
            let mut account = crate::github::Auth::connected_fixture();
            if let Some(profile) = &mut account.profile {
                profile.login = "remote-user".into();
            }
            view.menu.github_hosts.insert("ssh:test".into(), account);
            view.selected_endpoint = 1;
            // Selecting the remote moves the window's lookups to its view.
            let elsewhere = crate::pull_request::fixture().unwrap();
            view.seed_pr(input("elsewhere"), elsewhere, now);
            view
        });
        cx.observe(&view, |_, _, cx| cx.notify()).detach();
        SidebarFixture(view)
    });
    cx.simulate_resize(size(px(800.), px(600.)));
    cx.run_until_parked();
    cx.update(|window, cx| full_draw(window, cx).clear(cx));
    assert!(cx.debug_bounds("name-remote launcher").is_some());
    assert!(cx.debug_bounds("pr-agent-launcher").is_some());
    assert!(cx.debug_bounds("pr-remote launcher").is_none());
}
