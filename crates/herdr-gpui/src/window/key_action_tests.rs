//! Daemon key actions the window runs itself: renames, workspace closing,
//! and opening or removing worktrees open the dialogs their menus open,
//! detaching lets go of the daemon as the menu does, and resize mode claims
//! keys until it ends.

#![allow(clippy::unwrap_used)]

use super::HerdrWindow;
use crate::{
    controls::Command,
    menu::{Page, WorkspaceAction},
    sidebar::layout_tests::fixture_window,
    state::ConnectionStatus,
};
use gpui::{Keystroke, TestAppContext, VisualTestContext};
use herdr_client::protocol::ClientShellSnapshot;
use std::sync::Arc;

fn connected(window: &mut gpui::Window, cx: &mut gpui::Context<HerdrWindow>) -> HerdrWindow {
    let mut view = fixture_window(window, cx);
    let snapshot: ClientShellSnapshot = serde_json::from_str(include_str!(
        "../../../herdr-protocol/tests/fixtures/endpoint-snapshot-v1.json"
    ))
    .unwrap();
    view.live.snapshot = Some(Arc::new(snapshot));
    view.live.status = ConnectionStatus::Connected;
    view
}

fn run(view: &gpui::Entity<HerdrWindow>, command: Command, cx: &mut VisualTestContext) {
    cx.update(|window, cx| view.update(cx, |view, cx| view.command(command, window, cx)));
}

fn dismiss(view: &gpui::Entity<HerdrWindow>, cx: &mut VisualTestContext) {
    cx.update(|window, cx| view.update(cx, |view, cx| view.dismiss_menu(window, cx)));
}

#[gpui::test]
fn renames_and_close_open_the_focused_targets_dialogs(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(connected);
    run(&view, Command::RenameTab, cx);
    view.read_with(cx, |view, _| {
        assert_eq!(view.menu.page, Some(Page::RenameTab));
    });
    dismiss(&view, cx);

    run(&view, Command::RenamePane, cx);
    view.read_with(cx, |view, _| {
        assert_eq!(view.menu.page, Some(Page::RenamePane));
    });
    dismiss(&view, cx);

    for (command, action) in [
        (Command::RenameWorkspace, WorkspaceAction::Rename),
        (Command::CloseWorkspace, WorkspaceAction::Close),
    ] {
        run(&view, command, cx);
        view.read_with(cx, |view, _| {
            assert_eq!(view.menu.page, Some(Page::Dialog(action)), "{command:?}");
        });
        dismiss(&view, cx);
    }

    // Nothing focused, nothing to open.
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            let snapshot = Arc::make_mut(view.live.snapshot.as_mut().unwrap());
            snapshot.focused_workspace_id = None;
            snapshot.focused_tab_id = None;
            snapshot.focused_pane_id = None;
        })
    });
    for command in [
        Command::RenameTab,
        Command::RenamePane,
        Command::RenameWorkspace,
        Command::CloseWorkspace,
    ] {
        run(&view, command, cx);
        view.read_with(cx, |view, _| {
            assert_eq!(view.menu.page, None, "{command:?}")
        });
    }
}

#[gpui::test]
fn resize_mode_claims_keys_until_it_ends(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(connected);
    let press = |key: &str, cx: &mut VisualTestContext| {
        cx.update(|window, cx| window.dispatch_keystroke(Keystroke::parse(key).unwrap(), cx))
    };
    cx.update(|window, cx| {
        window.focus(&view.read(cx).focus.clone(), cx);
        window.draw(cx).clear(cx);
    });
    let resizing = |cx: &mut VisualTestContext| view.read_with(cx, |view, _| view.resize_mode);

    // Herdr's default chord enters it, and every key but cmd ones is claimed.
    assert!(press("ctrl-b", cx));
    assert!(press("r", cx));
    assert!(resizing(cx));
    for key in ["h", "down", "x", "shift-q"] {
        assert!(press(key, cx), "{key}");
        assert!(resizing(cx), "{key}");
    }
    assert!(!press("cmd-y", cx), "platform keys pass through");
    for end in ["escape", "enter", "r"] {
        assert!(press(end, cx), "{end}");
        assert!(!resizing(cx), "{end}");
        run(&view, Command::ResizeMode, cx);
        assert!(resizing(cx));
    }
    // A menu takes keys of its own, so opening one ends the mode.
    cx.update(|window, cx| view.update(cx, |view, cx| view.open_keybinds(window, cx)));
    press("h", cx);
    assert!(!resizing(cx));
    dismiss(&view, cx);
    // As does leaving the window.
    run(&view, Command::ResizeMode, cx);
    view.update(cx, |view, _| view.disarm_prefix());
    assert!(!resizing(cx));
}

/// `fixture_window`'s sidebar snapshot, connected, focused on `focused`: w3
/// is a repository's main checkout and w4 a linked checkout of it.
fn focus_worktree_fixture(
    view: &gpui::Entity<HerdrWindow>,
    focused: &str,
    cx: &mut VisualTestContext,
) {
    let focused = focused.to_owned();
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.menu.reset();
            view.flash = None;
            view.live.status = ConnectionStatus::Connected;
            let snapshot = Arc::make_mut(view.live.snapshot.as_mut().unwrap());
            snapshot.workspaces = crate::sidebar::layout_tests::snapshot(7).workspaces;
            snapshot.focused_workspace_id = Some(focused);
            window.focus(&view.focus, cx);
        })
    });
}

fn flash_text(view: &gpui::Entity<HerdrWindow>, cx: &mut VisualTestContext) -> Option<String> {
    view.read_with(cx, |view, _| {
        view.flash.as_ref().map(|(flash, _)| flash.text.to_string())
    })
}

/// `open_worktree` and `remove_worktree` open the focused workspace's menu
/// dialogs, so removal keeps its confirmation; a workspace without the row
/// opens nothing and says why.
#[gpui::test]
fn worktree_actions_open_the_focused_workspaces_dialogs(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(fixture_window);
    let shown = |cx: &mut VisualTestContext| {
        view.read_with(cx, |view, _| {
            (
                view.menu.page,
                crate::menu::workspace_tests::target_id(view).map(str::to_owned),
            )
        })
    };
    for (focused, command, action, target) in [
        (
            "w3",
            Command::OpenWorktree,
            WorkspaceAction::OpenWorktree,
            "w3",
        ),
        // A linked checkout opens through its main checkout.
        (
            "w4",
            Command::OpenWorktree,
            WorkspaceAction::OpenWorktree,
            "w3",
        ),
        (
            "w4",
            Command::RemoveWorktree,
            WorkspaceAction::DeleteWorktree,
            "w4",
        ),
    ] {
        focus_worktree_fixture(&view, focused, cx);
        run(&view, command, cx);
        assert_eq!(
            shown(cx),
            (Some(Page::Dialog(action)), Some(target.to_owned())),
            "{focused} {command:?}"
        );
        assert_eq!(flash_text(&view, cx), None);
    }

    for (focused, command, reason) in [
        (
            "w3",
            Command::RemoveWorktree,
            "This workspace is not a worktree checkout",
        ),
        (
            "w0",
            Command::RemoveWorktree,
            "This workspace is not a worktree checkout",
        ),
    ] {
        focus_worktree_fixture(&view, focused, cx);
        run(&view, command, cx);
        assert_eq!(shown(cx).0, None, "{focused} {command:?}");
        assert_eq!(flash_text(&view, cx).as_deref(), Some(reason));
    }

    focus_worktree_fixture(&view, "w4", cx);
    view.update(cx, |view, _| {
        view.live.status = ConnectionStatus::Disconnected
    });
    for command in [Command::OpenWorktree, Command::RemoveWorktree] {
        run(&view, command, cx);
        assert_eq!(shown(cx).0, None, "{command:?}");
        assert_eq!(
            flash_text(&view, cx).as_deref(),
            Some("Not connected, so no worktree can be changed")
        );
    }
}

/// `detach` lets go of the selected daemon as the settings menu's row does,
/// leaving it running, and says so when there is no connection to drop.
#[gpui::test]
fn detach_lets_go_of_the_selected_daemon(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(connected);
    run(&view, Command::Detach, cx);
    view.read_with(cx, |view, _| {
        assert_eq!(view.live.status, ConnectionStatus::Connected);
    });
    assert_eq!(
        flash_text(&view, cx).as_deref(),
        Some("Not connected, so there is nothing to detach")
    );

    view.update(cx, |view, _| {
        let endpoint = &mut view.endpoints[view.selected_endpoint];
        let client = herdr_client::connect(endpoint.connection.target.clone(), view.options)
            .unwrap_or_else(|error| panic!("cannot create test client: {error}"));
        client.handle.disconnect();
        endpoint.connection.handle = Some(client.handle);
    });
    run(&view, Command::Detach, cx);
    view.read_with(cx, |view, _| {
        let endpoint = &view.endpoints[view.selected_endpoint];
        assert!(endpoint.connection.handle.is_none());
        assert_eq!(view.live.status, ConnectionStatus::Detached);
    });
}
