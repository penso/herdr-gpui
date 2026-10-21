//! The workspace popover offers a Git workspace's issues on a host that runs
//! scripts, and shows them in the Issues section.
use super::*;

#[gpui::test]
fn a_git_workspace_offers_issues_and_pull_requests(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(sidebar::layout_tests::fixture_window);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.live.status = crate::state::ConnectionStatus::Connected;
            // An explicit socket cannot run scripts; the local session can.
            view.endpoints[0].connection.target = herdr_client::ConnectTarget::Local;
            view.open_workspace_menu("w3", Default::default(), window, cx);
        })
    });
    let items = cx.update(|_, cx| view.read(cx).workspace_items());
    assert!(
        items.iter().any(
            |(action, label)| *action == WorkspaceMenuAction::Orchestrator
                && *label == "Issues and pull requests"
        ),
        "{items:?}"
    );
    // A workspace that is not a Git checkout has nothing to list.
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.dismiss_menu(window, cx);
            view.open_workspace_menu("w1", Default::default(), window, cx);
        })
    });
    let items = cx.update(|_, cx| view.read(cx).workspace_items());
    assert!(
        !items
            .iter()
            .any(|(action, _)| *action == WorkspaceMenuAction::Orchestrator)
    );
}

#[gpui::test]
fn the_menu_shows_every_project_narrowed_to_the_workspaces(cx: &mut gpui::TestAppContext) {
    let repositories = tempfile::tempdir().unwrap();
    for folder in ["worktrees", "other"] {
        gix::init(repositories.path().join(folder)).unwrap();
    }
    let (view, cx) = cx.add_window_view(sidebar::layout_tests::fixture_window);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.config.sidebar_style.activity_bar = true;
            view.live.status = crate::state::ConnectionStatus::Connected;
            view.endpoints[0].connection.target = herdr_client::ConnectTarget::Local;
            if let Some(snapshot) = view.live.snapshot.as_mut() {
                std::sync::Arc::make_mut(snapshot).focused_workspace_id = Some("w3".into());
            }
            view.prepare_orchestrator_projects_for_tests(repositories.path(), cx);
            // A folder no workspace holds narrows to the workspace's project.
            view.open_orchestrator_in("w3".into(), Some("/nonexistent/repo".into()), window, cx);
            assert_eq!(view.activity.panel, sidebar::ActivityPanel::Issues);
            view.poll_orchestrators(window, cx);
            let id = view.activity.section_view.unwrap();
            let listing = view.orchestrators[&id].view().read(cx);
            let keys = listing.repo_keys();
            // The fixture's workspaces hold two repositories: agent-launcher's
            // worktrees, and the folder the others share.
            assert_eq!(keys.len(), 2, "{keys:?}");
            // The fixture's folders are not on disk: once detection says
            // so, which a fast machine may have done already, the pick is
            // dropped rather than hide the other project.
            let key = view.workspace_repo_key("w3").unwrap();
            let picked = listing.picked_projects();
            assert!(
                picked == [key.clone()] || (picked.is_empty() && listing.outside_git(&key)),
                "{picked:?}"
            );
        })
    });
    // Without the activity bar there is nowhere to show them, which is said.
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.activity.pick(sidebar::ActivityPanel::Workspaces);
            view.config.sidebar_style.activity_bar = false;
            view.open_orchestrator_in("w3".into(), None, window, cx);
            assert_eq!(view.activity.panel, sidebar::ActivityPanel::Workspaces);
        })
    });
}

#[gpui::test]
fn a_run_whose_workspace_closed_says_so_in_the_view(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(sidebar::layout_tests::fixture_window);
    let workspace = "w3".to_owned();
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            if let Some(snapshot) = view.live.snapshot.as_mut() {
                std::sync::Arc::make_mut(snapshot).focused_workspace_id = Some(workspace.clone());
            }
        })
    });
    let id = cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.config.sidebar_style.activity_bar = true;
            view.endpoints[0].connection.target = herdr_client::ConnectTarget::Local;
            view.open_orchestrator_in(
                workspace.clone(),
                Some("/nonexistent/orchestrator-test".into()),
                window,
                cx,
            );
            view.activity.section_view.unwrap()
        })
    });
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.poll_orchestrators(window, cx);
            // The run's workspace is not on any connected host.
            view.orchestrator_events.push((
                id,
                crate::orchestrator::Event::OpenWorkspace {
                    host: None,
                    session: None,
                    workspace_id: "w404".into(),
                },
            ));
            view.poll_orchestrators(window, cx);
        })
    });
    let text = cx.update(|_, cx| {
        view.read(cx).orchestrators[&id]
            .view()
            .read(cx)
            .notice_text()
    });
    assert_eq!(text.as_deref(), Some("This run's workspace is closed"));
}
