#![allow(clippy::unwrap_used)]
use super::*;
use gpui::TestAppContext;

#[gpui::test]
fn the_phone_gets_the_local_desktop_cache_and_loses_a_badge_after_a_branch_change(
    cx: &mut TestAppContext,
) {
    let (view, cx) = cx.add_window_view(crate::sidebar::layout_tests::fixture_window);
    let mut status = super::super::tests::status(0);
    status.boot_id = "layout-test".into();
    cx.update(|_, cx| {
        cx.set_global(Supervisor {
            health: Health::Running(status),
            config: RemoteConfig {
                enabled: true,
                ..Default::default()
            },
            ..Supervisor::default()
        })
    });
    view.update(cx, |view, cx| {
        view.live.status = crate::state::ConnectionStatus::Connected;
        let workspace = view.live.snapshot.as_ref().unwrap().workspaces[4].clone();
        let input = crate::pull_request::workspace_input(&workspace, &Origin::Local).unwrap();
        let mut pr = crate::pull_request::fixture().unwrap();
        pr.number = 850;
        let tone = pr.tone();
        view.seed_pr(input, pr, Instant::now());
        view.publish_remote_pull_requests(cx);
        let sent = cx.global::<Supervisor>().pull_requests.as_ref().unwrap();
        let badge = sent
            .entries
            .iter()
            .find(|entry| entry.workspace_id == workspace.workspace_id)
            .unwrap();
        assert_eq!(badge.number, 850);
        assert_eq!(badge.tone, tone);
        assert_eq!(badge.cwd, workspace.new_workspace_cwd);
        assert_eq!(sent.boot_id, view.live.snapshot.as_ref().unwrap().boot_id);
        Arc::make_mut(view.live.snapshot.as_mut().unwrap()).workspaces[4].branch =
            Some("another-branch".into());
        cx.global_mut::<Supervisor>().pull_requests_checked = None;
        view.publish_remote_pull_requests(cx);
        assert!(
            !cx.global::<Supervisor>()
                .pull_requests
                .as_ref()
                .unwrap()
                .entries
                .iter()
                .any(|entry| entry.workspace_id == workspace.workspace_id)
        );
        // An unverified endpoint must never publish another host's PR cache.
        view.live.local_daemon_peer = false;
        let supervisor = cx.global_mut::<Supervisor>();
        supervisor.pull_requests_checked = None;
        supervisor.pull_requests = None;
        view.publish_remote_pull_requests(cx);
        assert!(cx.global::<Supervisor>().pull_requests.is_none());
    });
}

#[gpui::test]
fn the_phone_hears_of_commits_without_a_pull_request_until_one_opens(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(crate::sidebar::layout_tests::fixture_window);
    let mut status = super::super::tests::status(0);
    status.boot_id = "layout-test".into();
    cx.update(|_, cx| {
        cx.set_global(Supervisor {
            health: Health::Running(status),
            config: RemoteConfig {
                enabled: true,
                ..Default::default()
            },
            ..Supervisor::default()
        })
    });
    view.update(cx, |view, cx| {
        view.live.status = crate::state::ConnectionStatus::Connected;
        let workspace = view.live.snapshot.as_ref().unwrap().workspaces[4].clone();
        let input = crate::pull_request::workspace_input(&workspace, &Origin::Local).unwrap();
        let now = Instant::now();
        let ahead = crate::git::Ahead {
            commits: 3,
            head: "tip".into(),
        };
        view.git.seed_ahead(input.clone(), ahead, now);
        view.seed_no_pr(input.clone(), now);
        view.publish_remote_pull_requests(cx);
        let sent = cx.global::<Supervisor>().pull_requests.as_ref().unwrap();
        assert!(sent.entries.is_empty());
        let waiting = sent.unopened.first().unwrap();
        assert_eq!(waiting.workspace_id, workspace.workspace_id);
        assert_eq!(waiting.cwd, workspace.new_workspace_cwd);
        assert_eq!(waiting.commits, 3);

        let mut pr = crate::pull_request::fixture().unwrap();
        pr.state = crate::pull_request::State::Open;
        view.seed_pr(input, pr, now);
        cx.global_mut::<Supervisor>().pull_requests_checked = None;
        view.publish_remote_pull_requests(cx);
        let sent = cx.global::<Supervisor>().pull_requests.as_ref().unwrap();
        assert!(sent.unopened.is_empty());
        assert_eq!(sent.entries.len(), 1);
    });
}
