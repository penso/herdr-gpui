use super::super::worktree_shortcut::Unavailable;
use super::*;
use crate::{
    controls::Command,
    keymap::{DaemonKeys, Keymap},
    state::ConnectionStatus,
};
use gpui::{KeyDownEvent, Keystroke, PlatformInput};

/// Herdr leaves both actions unbound, so the window answers what a user's
/// `[keys]` binds them to: a prefix chord and a direct key.
fn window_with_worktree_keys(
    cx: &mut gpui::TestAppContext,
) -> (Entity<SheprdWindow>, &mut VisualTestContext) {
    let (view, cx) = cx.add_window_view(sidebar::layout_tests::fixture_window);
    cx.simulate_resize(gpui::size(gpui::px(900.), gpui::px(700.)));
    let table: toml::Table =
        "[keys]\nopen_worktree = 'prefix+shift+o'\nremove_worktree = 'ctrl+alt+x'"
            .parse()
            .unwrap();
    let keymap = Keymap::with_overrides(
        &Default::default(),
        &Default::default(),
        &DaemonKeys::from_table(table.get("keys").and_then(toml::Value::as_table)),
    )
    .unwrap();
    view.update(cx, |view, _| view.config.keybindings = keymap.clone());
    // Direct keys are app-wide bindings, taken from the active daemon's keymap.
    cx.update(|_, cx| {
        cx.set_global(crate::window::ActiveServerKeymap(Some(keymap)));
        crate::bind_keys(cx);
    });
    (view, cx)
}

/// Resets the fixture to a connected window focused on `focused`, then lets
/// `setup` take something away.
fn prepare(
    view: &Entity<SheprdWindow>,
    cx: &mut VisualTestContext,
    focused: &str,
    setup: fn(&mut SheprdWindow),
) {
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            let snapshot = std::sync::Arc::make_mut(view.live.snapshot.as_mut().unwrap());
            snapshot.workspaces = sidebar::layout_tests::snapshot(7).workspaces;
            snapshot.focused_workspace_id = Some(focused.into());
            view.live.status = ConnectionStatus::Connected;
            view.live.local_daemon_peer = true;
            view.menu.reset();
            view.removal = None;
            view.flash = None;
            setup(view);
            window.focus(&view.focus, cx);
        });
    });
    draw(cx);
}

fn press(command: Command, cx: &mut VisualTestContext) {
    let keys: &[&str] = match command {
        Command::OpenWorktree => &["ctrl-b", "shift-o"],
        _ => &["ctrl-alt-x"],
    };
    cx.update(|window, cx| {
        for key in keys {
            assert!(
                window.dispatch_keystroke(crate::keymap::parse_keystroke(key).unwrap(), cx),
                "{key}"
            );
        }
    });
    draw(cx);
}

fn not_git(view: &mut SheprdWindow) {
    let snapshot = std::sync::Arc::make_mut(view.live.snapshot.as_mut().unwrap());
    let w1 = snapshot
        .workspaces
        .iter_mut()
        .find(|w| w.workspace_id == "w1")
        .unwrap();
    w1.branch = None;
    w1.worktree = None;
}

/// Each key opens the dialog the focused workspace's menu row would: a linked
/// checkout opens worktrees through its repository's main checkout, and
/// removes only itself.
#[gpui::test]
fn worktree_keys_open_the_focused_workspace_dialogs(cx: &mut gpui::TestAppContext) {
    let (view, cx) = window_with_worktree_keys(cx);
    for (command, focused, expected, action) in [
        (
            Command::OpenWorktree,
            "w3",
            "w3",
            WorkspaceAction::OpenWorktree,
        ),
        (
            Command::OpenWorktree,
            "w4",
            "w3",
            WorkspaceAction::OpenWorktree,
        ),
        // A branch reported before worktree metadata is a main checkout too.
        (
            Command::OpenWorktree,
            "w1",
            "w1",
            WorkspaceAction::OpenWorktree,
        ),
        (
            Command::RemoveWorktree,
            "w4",
            "w4",
            WorkspaceAction::DeleteWorktree,
        ),
        (
            Command::RemoveWorktree,
            "w5",
            "w5",
            WorkspaceAction::DeleteWorktree,
        ),
    ] {
        prepare(&view, cx, focused, |_| {});
        press(command, cx);
        cx.update(|_, cx| {
            let view = view.read(cx);
            assert_eq!(view.menu.page, Some(Page::Dialog(action)), "{focused}");
            assert_eq!(
                view.menu.target.as_ref().map(|target| target.id.as_str()),
                Some(expected),
                "{focused}"
            );
            assert!(view.flash.is_none(), "{focused}");
            // Removal waits on its confirmation, sending nothing yet.
            assert_eq!(
                view.menu.deletion.is_some(),
                action == WorkspaceAction::DeleteWorktree
            );
            assert!(view.removal.is_none());
        });
    }

    // The removal confirmation still wants a fresh Enter: a repeat of a held
    // key cannot confirm it.
    cx.update(|window, cx| {
        window.dispatch_event(
            PlatformInput::KeyDown(KeyDownEvent {
                keystroke: Keystroke::parse("enter").unwrap(),
                is_held: true,
                prefer_character_input: false,
            }),
            cx,
        );
    });
    cx.update(|_, cx| {
        let view = view.read(cx);
        assert_eq!(
            view.menu.page,
            Some(Page::Dialog(WorkspaceAction::DeleteWorktree))
        );
        assert!(view.removal.is_none());
    });
}

/// Without a workspace the key could act on, nothing opens and a flash says
/// why rather than the key doing nothing visible.
#[gpui::test]
fn worktree_keys_refuse_with_a_reason(cx: &mut gpui::TestAppContext) {
    use Unavailable::*;
    let (view, cx) = window_with_worktree_keys(cx);
    type Setup = fn(&mut SheprdWindow);
    let no_focus: Setup = |view| {
        std::sync::Arc::make_mut(view.live.snapshot.as_mut().unwrap()).focused_workspace_id = None
    };
    let disconnected: Setup = |view| view.live.status = ConnectionStatus::Disconnected;
    let main_closed: Setup = |view| {
        std::sync::Arc::make_mut(view.live.snapshot.as_mut().unwrap())
            .workspaces
            .retain(|w| w.workspace_id != "w3")
    };
    let cases: [(Command, &str, Setup, Unavailable); 10] = [
        (Command::OpenWorktree, "w1", not_git, NotGit),
        (Command::RemoveWorktree, "w1", not_git, NotGit),
        (Command::OpenWorktree, "w4", main_closed, MainCheckoutClosed),
        (Command::RemoveWorktree, "w3", |_| {}, MainCheckout),
        (Command::RemoveWorktree, "w1", |_| {}, MainCheckout),
        (Command::OpenWorktree, "w3", no_focus, NoWorkspace),
        (Command::RemoveWorktree, "w4", no_focus, NoWorkspace),
        (Command::OpenWorktree, "w3", disconnected, Disconnected),
        (Command::RemoveWorktree, "w4", disconnected, Disconnected),
        (
            Command::RemoveWorktree,
            "w4",
            |view| view.live.snapshot = None,
            Disconnected,
        ),
    ];
    for (command, focused, setup, reason) in cases {
        prepare(&view, cx, focused, setup);
        press(command, cx);
        cx.update(|_, cx| {
            let view = view.read(cx);
            assert!(view.menu.page.is_none(), "{command:?} {reason:?}");
            assert!(view.removal.is_none());
            assert_eq!(
                view.flash.as_ref().map(|(flash, _)| flash),
                Some(&crate::window::Flash::warning(reason.message())),
                "{command:?}"
            );
        });
        assert!(cx.debug_bounds("flash").is_some(), "{reason:?}");
    }
}
