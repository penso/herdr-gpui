use super::*;
use crate::notification_mutes::{Daemon, Mutes};
use gpui::{
    Element, IntoElement,
    accesskit::{Node, Role},
};

#[gpui::test]
fn the_header_toggle_names_what_it_does(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(sidebar::layout_tests::fixture_window);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            for (action, name) in [
                (WorkspaceMenuAction::Mute, "Mute notifications"),
                (WorkspaceMenuAction::Unmute, "Unmute notifications"),
            ] {
                let element = view.render_workspace_mute(action, name, cx).into_element();
                let mut node = Node::new(element.a11y_role().unwrap());
                element.write_a11y_info(&mut node);
                assert_eq!(node.role(), Role::Button);
                assert_eq!(node.label(), Some(name));
            }
        })
    });
}

/// The menu reads the mute when it opens, and its toggle applies the
/// opposite at once, only while the connection it opened on is current.
#[gpui::test]
fn the_toggle_mutes_what_the_menu_opened_on(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(|window, cx| {
        let mut view = sidebar::layout_tests::fixture_window(window, cx);
        view.live.status = crate::state::ConnectionStatus::Connected;
        view
    });
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            let daemon = Daemon::of(&view.endpoints[0]);
            let snapshot = view.live.snapshot.clone().unwrap();
            let muted = |cx: &gpui::App| {
                Mutes::of(cx)
                    .is_some_and(|mutes| mutes.silenced(&daemon, &snapshot).workspace("w3"))
            };
            view.open_workspace_menu("w3", Default::default(), window, cx);
            assert_eq!(view.menu.target.as_ref().map(|t| t.muted), Some(false));
            view.activate_workspace_menu(WorkspaceMenuAction::Mute, window, cx);
            assert!(view.menu.page.is_none());
            assert!(muted(cx));

            view.open_workspace_menu("w3", Default::default(), window, cx);
            assert_eq!(view.menu.target.as_ref().map(|t| t.muted), Some(true));
            // A menu from before a reconnection changes nothing.
            view.endpoints[0].generation += 1;
            view.activate_workspace_menu(WorkspaceMenuAction::Unmute, window, cx);
            assert!(muted(cx));
            view.endpoints[0].generation -= 1;
            view.open_workspace_menu("w3", Default::default(), window, cx);
            view.activate_workspace_menu(WorkspaceMenuAction::Unmute, window, cx);
            assert!(!muted(cx));
        })
    });
}

/// The toggle leads the arrow keys' walk, though no row draws it.
#[gpui::test]
fn the_arrow_keys_reach_the_toggle_first(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(|window, cx| {
        crate::bind_keys(cx);
        let mut view = sidebar::layout_tests::fixture_window(window, cx);
        view.live.status = crate::state::ConnectionStatus::Connected;
        view
    });
    for (action, label) in [
        (WorkspaceMenuAction::Mute, "Mute notifications"),
        (WorkspaceMenuAction::Unmute, "Unmute notifications"),
    ] {
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                view.open_workspace_menu("w3", Default::default(), window, cx);
                assert_eq!(view.workspace_menu_actions().first(), Some(&action));
                let (_, rest) = crate::menu::workspace::popover::layout(&view.workspace_items());
                assert!(!rest.iter().any(|(action, _)| action.in_header()));
            })
        });
        cx.update(|window, cx| {
            window.draw(cx).clear(cx);
        });
        let toggle = format!("workspace-menu-{label}").leak();
        assert!(cx.debug_bounds(toggle).is_some(), "{label}");
        cx.simulate_keystrokes("down");
        cx.update(|_, cx| assert_eq!(view.read(cx).menu.workspace_selected, Some(action)));
        cx.simulate_keystrokes("enter");
        cx.update(|_, cx| assert!(view.read(cx).menu.page.is_none()));
    }
    cx.update(|_, cx| {
        let view = view.read(cx);
        let snapshot = view.live.snapshot.clone().unwrap();
        let daemon = Daemon::of(&view.endpoints[0]);
        assert!(
            Mutes::of(cx).is_none_or(|mutes| !mutes.silenced(&daemon, &snapshot).workspace("w3")),
            "muted, then unmuted"
        );
    });
}
