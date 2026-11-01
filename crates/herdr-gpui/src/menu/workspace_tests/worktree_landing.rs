use super::*;
use crate::{
    teleport::AgentKind,
    worktree_landing::{Landing, Remembered},
};

/// The dialog offers the landing chosen last, and creating a worktree
/// remembers the one chosen this time for the next dialog.
#[gpui::test]
fn new_worktrees_offer_and_remember_the_last_landing(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(sidebar::layout_tests::fixture_window);
    cx.update(|window, cx| {
        Remembered::record(Landing::Agent(AgentKind::Claude), cx);
        view.update(cx, |view, cx| {
            let snapshot = std::sync::Arc::make_mut(view.live.snapshot.as_mut().unwrap());
            snapshot.workspaces = sidebar::layout_tests::snapshot(7).workspaces;
            view.live.status = crate::state::ConnectionStatus::Connected;
            view.open_workspace_menu("w3", Default::default(), window, cx);
            view.open_workspace_dialog(WorkspaceAction::NewWorktree, window, cx);
            // A bare socket names no host to start agents on.
            assert!(view.menu.landing.is_none());
            view.dismiss_menu(window, cx);
            view.endpoints[0].connection.target = herdr_client::ConnectTarget::Local;
            view.open_workspace_menu("w3", Default::default(), window, cx);
            view.open_workspace_dialog(WorkspaceAction::NewWorktree, window, cx);
            let picker = view.menu.landing.as_mut().unwrap();
            assert_eq!(picker.chosen, Landing::Agent(AgentKind::Claude));
            // The terminal is always offered, so starting no agent is a pick too.
            assert!(picker.choose(Landing::Terminal));

            // As submitting does once the creation request is queued.
            view.arm_landing("create", false);
            view.menu.creation = Some("create".into());
            let created = serde_json::json!({"result":{"type":"worktree_created",
                "workspace":{"workspace_id":"w6"},"tab":{"tab_id":"t9"},"root_pane":{"pane_id":"p9"}}});
            view.live.dialog_response = Some(("create".into(), Some(Ok(created))));
            view.update_workspace_dialog(window, cx);
            assert!(view.menu.page.is_none());
            assert!(view.menu.landing.is_none());
        });
        assert_eq!(Remembered::landing(cx), Landing::Terminal);
    });
}

/// Opening an existing checkout keeps whatever it runs, and its choice.
#[gpui::test]
fn opening_a_worktree_neither_lands_nor_remembers(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(sidebar::layout_tests::fixture_window);
    cx.update(|window, cx| {
        Remembered::record(Landing::Agent(AgentKind::Codex), cx);
        view.update(cx, |view, cx| {
            let snapshot = std::sync::Arc::make_mut(view.live.snapshot.as_mut().unwrap());
            snapshot.workspaces = sidebar::layout_tests::snapshot(7).workspaces;
            view.live.status = crate::state::ConnectionStatus::Connected;
            view.open_workspace_menu("w3", Default::default(), window, cx);
            view.open_workspace_dialog(WorkspaceAction::OpenWorktree, window, cx);
            assert!(view.menu.landing.is_none());
        });
        assert_eq!(Remembered::landing(cx), Landing::Agent(AgentKind::Codex));
    });
}

/// Closing the dialog while the daemon is still creating the checkout keeps
/// the choice: the answer that arrives afterwards still lands, and is
/// remembered for the next dialog.
#[gpui::test]
fn closing_the_dialog_during_creation_still_lands(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(sidebar::layout_tests::fixture_window);
    cx.update(|window, cx| {
        Remembered::record(Landing::Agent(AgentKind::Claude), cx);
        view.update(cx, |view, cx| {
            let snapshot = std::sync::Arc::make_mut(view.live.snapshot.as_mut().unwrap());
            snapshot.workspaces = sidebar::layout_tests::snapshot(7).workspaces;
            view.live.status = crate::state::ConnectionStatus::Connected;
            view.endpoints[0].connection.target = herdr_client::ConnectTarget::Local;
            view.open_workspace_menu("w3", Default::default(), window, cx);
            view.open_workspace_dialog(WorkspaceAction::NewWorktree, window, cx);
            // Terminal, so landing starts no agent on this machine.
            assert!(
                view.menu
                    .landing
                    .as_mut()
                    .unwrap()
                    .choose(Landing::Terminal)
            );
            view.arm_landing("create", false);
            view.menu.creation = Some("create".into());
            view.live.landing_response = Some(("create".into(), None));
            view.dismiss_menu(window, cx);
            assert!(view.menu.creation.is_none());
            assert!(view.landing_launch.is_some());

            // A newer dialog's request takes the dialog's slot, not the landing's.
            let created = serde_json::json!({"result":{"type":"worktree_created",
                "workspace":{"workspace_id":"w6"},"root_pane":{"pane_id":"p9"}}});
            view.live.dialog_response = Some(("list".into(), None));
            view.update_workspace_dialog(window, cx);
            assert!(view.landing_launch.is_some());

            view.live.landing_response = Some(("create".into(), Some(Ok(created))));
            view.update_workspace_dialog(window, cx);
            assert!(view.landing_launch.is_none());
            assert!(view.live.landing_response.is_none());
        });
        assert_eq!(Remembered::landing(cx), Landing::Terminal);
    });
}

/// A landing armed before a reconnect is dropped rather than applied to
/// whatever the new connection answers.
#[gpui::test]
fn a_stale_landing_is_dropped(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(sidebar::layout_tests::fixture_window);
    cx.update(|window, cx| {
        Remembered::record(Landing::Agent(AgentKind::Codex), cx);
        view.update(cx, |view, cx| {
            let snapshot = std::sync::Arc::make_mut(view.live.snapshot.as_mut().unwrap());
            snapshot.workspaces = sidebar::layout_tests::snapshot(7).workspaces;
            view.live.status = crate::state::ConnectionStatus::Connected;
            view.endpoints[0].connection.target = herdr_client::ConnectTarget::Local;
            view.open_workspace_menu("w3", Default::default(), window, cx);
            view.open_workspace_dialog(WorkspaceAction::NewWorktree, window, cx);
            view.arm_landing("create", false);
            view.dismiss_menu(window, cx);
            view.endpoints[0].generation += 1;
            let created = serde_json::json!({"result":{"type":"worktree_created",
                "workspace":{"workspace_id":"w6"},"root_pane":{"pane_id":"p9"}}});
            view.live.landing_response = Some(("create".into(), Some(Ok(created))));
            view.update_workspace_dialog(window, cx);
            assert!(view.landing_launch.is_none());
        });
        assert_eq!(Remembered::landing(cx), Landing::Agent(AgentKind::Codex));
    });
}

/// Opening an existing checkout arms no landing.
#[gpui::test]
fn opening_arms_no_landing(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(sidebar::layout_tests::fixture_window);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            let snapshot = std::sync::Arc::make_mut(view.live.snapshot.as_mut().unwrap());
            snapshot.workspaces = sidebar::layout_tests::snapshot(7).workspaces;
            view.live.status = crate::state::ConnectionStatus::Connected;
            view.endpoints[0].connection.target = herdr_client::ConnectTarget::Local;
            view.open_workspace_menu("w3", Default::default(), window, cx);
            view.open_workspace_dialog(WorkspaceAction::NewWorktree, window, cx);
            view.arm_landing("open", true);
            assert!(view.landing_launch.is_none());
        });
    });
}
