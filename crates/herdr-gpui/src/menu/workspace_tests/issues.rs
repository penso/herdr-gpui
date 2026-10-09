//! The workspace popover opens a Git workspace's Issues & PRs tab on a host
//! that runs scripts, and a restored orchestrator tab gets its view back.
use super::*;
use crate::browser::{Location, OrchestratorRepo, Store, scope};

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
            |(action, label)| *action == WorkspaceMenuAction::IssuesAndPullRequests
                && *label == "Issues & PRs"
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
            .any(|(action, _)| *action == WorkspaceMenuAction::IssuesAndPullRequests)
    );
}

#[gpui::test]
fn a_restored_orchestrator_tab_gets_its_view_and_a_closed_one_loses_it(
    cx: &mut gpui::TestAppContext,
) {
    let (view, cx) = cx.add_window_view(sidebar::layout_tests::fixture_window);
    let workspace = "w3".to_owned();
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            if let Some(snapshot) = view.live.snapshot.as_mut() {
                std::sync::Arc::make_mut(snapshot).focused_workspace_id = Some(workspace.clone());
            }
        })
    });
    let id = cx.update(|_, cx| {
        let tab_scope = scope(&view.read(cx).endpoints[0]);
        Store::update(cx, |store| {
            store.open(
                tab_scope,
                &workspace,
                Some(Location::Orchestrator {
                    repo: OrchestratorRepo::new("/nonexistent/orchestrator-test".into()).unwrap(),
                }),
                None,
            )
        })
        .unwrap()
    });
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            assert!(view.orchestrators.is_empty());
            view.poll_orchestrators(window, cx);
            assert!(view.orchestrators.contains_key(&id));
            view.poll_orchestrators(window, cx);
            assert_eq!(view.orchestrators.len(), 1);
        })
    });
    cx.update(|window, cx| {
        Store::update(cx, |store| store.close(id));
        view.update(cx, |view, cx| {
            view.poll_orchestrators(window, cx);
            assert!(view.orchestrators.is_empty());
        })
    });
}

#[gpui::test]
fn a_view_moves_into_its_own_window_and_goes_when_it_closes(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(sidebar::layout_tests::fixture_window);
    let workspace = "w3".to_owned();
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            if let Some(snapshot) = view.live.snapshot.as_mut() {
                std::sync::Arc::make_mut(snapshot).focused_workspace_id = Some(workspace.clone());
            }
        })
    });
    let id = cx.update(|_, cx| {
        let tab_scope = scope(&view.read(cx).endpoints[0]);
        Store::update(cx, |store| {
            store.open(
                tab_scope,
                &workspace,
                Some(Location::Orchestrator {
                    repo: OrchestratorRepo::new("/nonexistent/orchestrator-test".into()).unwrap(),
                }),
                None,
            )
        })
        .unwrap()
    });
    cx.update(|window, cx| view.update(cx, |view, cx| view.poll_orchestrators(window, cx)));
    cx.update(|_, cx| view.update(cx, |view, cx| view.detach_orchestrator(id, cx)));
    cx.update(|window, cx| {
        assert!(
            Store::update(cx, |store| store.get(id).is_none()),
            "the tab closed"
        );
        view.update(cx, |view, cx| {
            assert!(view.orchestrators.is_empty());
            assert_eq!(view.detached_orchestrators.len(), 1);
            // Its window keeps it alive across ticks.
            view.poll_orchestrators(window, cx);
            assert_eq!(view.detached_orchestrators.len(), 1);
        })
    });
    let handle = cx.update(|_, cx| view.read(cx).detached_orchestrators[&id].window());
    cx.update(|_, cx| {
        handle
            .update(cx, |_, window, _| window.remove_window())
            .unwrap();
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.poll_orchestrators(window, cx);
            assert!(view.detached_orchestrators.is_empty());
        })
    });
}
