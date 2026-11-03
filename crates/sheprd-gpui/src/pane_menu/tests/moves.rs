use super::*;
use crate::pane_move::{self, Destination};

/// Moves run the `herdr` CLI, so they are offered only where it can run,
/// list every destination, refuse a destination that vanished, and let
/// only the move a menu is waiting for settle it.
#[gpui::test]
fn pane_moves_offer_destinations_and_settle_once(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(|window, cx| {
        crate::bind_keys(cx);
        let mut view = crate::sidebar::layout_tests::fixture_window(window, cx);
        view.live.snapshot = Some(Arc::new(snapshot()));
        view
    });
    cx.simulate_resize(size(px(800.), px(600.)));
    let is_move = |action: &Action| {
        matches!(
            action,
            Action::MoveToNewTab | Action::MoveToNewWorkspace | Action::MoveTo
        )
    };
    cx.update(|window, cx| {
        view.update(cx, |v, cx| {
            // The fixture's custom socket has no host for the CLI.
            v.open_pane_menu("inactive", Point::default(), window, cx);
            let pane = v.menu.pane.as_ref().unwrap();
            assert!(!pane.movable);
            assert!(!pane.actions().iter().any(is_move));
            v.dismiss_menu(window, cx);
            v.move_focused_pane(window, cx);
            assert_eq!(v.menu.page, Some(Page::Pane));
            assert!(
                v.menu
                    .pane
                    .as_ref()
                    .unwrap()
                    .error
                    .as_ref()
                    .unwrap()
                    .contains("custom socket")
            );
            v.dismiss_menu(window, cx);

            v.endpoints[0].connection.target = sheprd_client::ConnectTarget::Local;
            v.open_pane_menu("inactive", Point::default(), window, cx);
            // One tab in one workspace: only the new places.
            let actions = v.menu.pane.as_ref().unwrap().actions();
            assert_eq!(actions.iter().filter(|a| is_move(a)).count(), 2);
            v.dismiss_menu(window, cx);

            let mut snapshot = snapshot();
            let mut logs = snapshot.tabs[0].clone();
            logs.tab_id = "w1:t2".into();
            logs.number = 2;
            logs.label = "logs".into();
            snapshot.tabs.push(logs);
            v.live.snapshot = Some(Arc::new(snapshot));
            v.open_pane_menu("inactive", Point::default(), window, cx);
            assert_eq!(
                v.menu
                    .pane
                    .as_ref()
                    .unwrap()
                    .actions()
                    .iter()
                    .filter(|a| is_move(a))
                    .count(),
                3
            );
            v.activate_pane_menu(Action::MoveTo, window, cx);
            assert_eq!(v.menu.page, Some(Page::MovePane));
            let labels: Vec<_> = v
                .menu
                .pane
                .as_ref()
                .unwrap()
                .target
                .destinations()
                .into_iter()
                .map(|choice| (choice.workspace, choice.place))
                .collect();
            assert_eq!(
                labels,
                [
                    (None, "New Tab".to_owned()),
                    (None, "New Workspace".to_owned()),
                    (Some("repo".to_owned()), "logs".to_owned()),
                ]
            );
        });
        window.draw(cx).clear(cx);
    });
    for selector in ["pane-move-0", "pane-move-1", "pane-move-2"] {
        assert!(cx.debug_bounds(selector).is_some());
    }
    assert!(cx.debug_bounds("pane-move-3").is_none());
    cx.simulate_keystrokes("down down up");
    view.read_with(cx, |v, _| {
        assert_eq!(v.menu.pane.as_ref().unwrap().selected, Some(0));
    });
    cx.update(|window, cx| {
        view.update(cx, |v, cx| {
            // The tab closed after the picker opened: nothing runs.
            Arc::make_mut(v.live.snapshot.as_mut().unwrap())
                .tabs
                .retain(|tab| tab.tab_id != "w1:t2");
            v.move_pane(
                Destination::Tab {
                    tab_id: "w1:t2".into(),
                },
                window,
                cx,
            );
            let pane = v.menu.pane.as_ref().unwrap();
            assert!(pane.moving.is_none());
            assert_eq!(
                pane.error.as_deref(),
                Some(crate::Error::StaleTab.to_string().as_str())
            );

            // While a move runs, the menu starts no other.
            v.menu.pane.as_mut().unwrap().moving = Some(7);
            v.activate_pane_menu(Action::MoveToNewWorkspace, window, cx);
            v.move_pane(Destination::NewWorkspace, window, cx);
            assert_eq!(v.menu.pane.as_ref().unwrap().moving, Some(7));
            // Another move's late failure reaches the window, not this menu.
            v.local_error = None;
            v.finish_pane_move(
                6,
                Err(pane_move::Error::Unchanged(pane_move::Unchanged::SameTab)),
                window,
                cx,
            );
            assert_eq!(v.menu.pane.as_ref().unwrap().moving, Some(7));
            assert_eq!(
                v.local_error.as_deref(),
                Some("The pane is already in that tab.")
            );
            v.finish_pane_move(
                7,
                Err(pane_move::Error::Unchanged(pane_move::Unchanged::ZoomedTab)),
                window,
                cx,
            );
            let pane = v.menu.pane.as_ref().unwrap();
            assert!(pane.moving.is_none());
            assert!(pane.error.as_ref().unwrap().contains("Unzoom"));
            assert_eq!(v.menu.page, Some(Page::MovePane));
            v.menu.pane.as_mut().unwrap().moving = Some(8);
            v.finish_pane_move(8, Ok(()), window, cx);
            assert!(v.menu.page.is_none());
            // A success after the menu closed changes nothing visible.
            v.local_error = None;
            v.finish_pane_move(9, Ok(()), window, cx);
            assert!(v.local_error.is_none());
        })
    });
}

/// Double-clicking "Move to…" opens the picker and nothing more: the
/// second click lands on whichever picker row is now under the pointer, and
/// must not move the pane there.
#[gpui::test]
fn double_click_into_the_picker_moves_nothing(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(|window, cx| {
        crate::bind_keys(cx);
        let mut view = crate::sidebar::layout_tests::fixture_window(window, cx);
        let mut snapshot = snapshot();
        let mut logs = snapshot.tabs[0].clone();
        logs.tab_id = "w1:t2".into();
        logs.number = 2;
        logs.label = "logs".into();
        snapshot.tabs.push(logs);
        view.live.snapshot = Some(Arc::new(snapshot));
        // A session that does not exist, so a move the test failed to stop
        // could only be refused, never reach a real daemon.
        view.endpoints[0].connection.target = sheprd_client::ConnectTarget::Session {
            name: "move-test-no-such-session".into(),
            development: false,
        };
        view
    });
    cx.simulate_resize(size(px(800.), px(600.)));
    let index = cx.update(|window, cx| {
        view.update(cx, |v, cx| {
            v.open_pane_menu("inactive", point(px(100.), px(100.)), window, cx);
            v.menu
                .pane
                .as_ref()
                .unwrap()
                .actions()
                .iter()
                .position(|action| matches!(action, Action::MoveTo))
                .unwrap()
        })
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let selector: &'static str = format!("pane-menu-{index}").leak();
    let row = cx.debug_bounds(selector).unwrap().center();
    cx.simulate_click(row, Modifiers::default());
    cx.update(|window, cx| window.draw(cx).clear(cx));
    // The second click of the double-click, on the first picker row.
    let picked = cx.debug_bounds("pane-move-0").unwrap().center();
    cx.simulate_event(MouseDownEvent {
        position: picked,
        button: MouseButton::Left,
        modifiers: Modifiers::default(),
        click_count: 2,
        first_mouse: false,
    });
    cx.simulate_event(MouseUpEvent {
        position: picked,
        button: MouseButton::Left,
        modifiers: Modifiers::default(),
        click_count: 2,
    });
    view.read_with(cx, |v, _| {
        assert_eq!(v.menu.page, Some(Page::MovePane));
        let pane = v.menu.pane.as_ref().unwrap();
        assert!(pane.moving.is_none() && pane.error.is_none());
    });
    // A deliberate single click on the same row still reaches the move. A
    // restarted daemon makes it stale, so it is refused before any process
    // runs.
    cx.update(|_, cx| {
        view.update(cx, |v, _| {
            Arc::make_mut(v.live.snapshot.as_mut().unwrap()).boot_id = "restarted".into();
        })
    });
    cx.simulate_click(picked, Modifiers::default());
    view.read_with(cx, |v, _| {
        let pane = v.menu.pane.as_ref().unwrap();
        assert!(pane.moving.is_none());
        assert_eq!(
            pane.error.as_deref(),
            Some(crate::Error::StalePane.to_string().as_str())
        );
    });
}

/// A workspace name too long for the picker gives way to the tab: every row
/// of one workspace still shows which tab it names.
#[gpui::test]
fn long_workspace_names_leave_the_tab_readable(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(|window, cx| {
        crate::bind_keys(cx);
        let mut view = crate::sidebar::layout_tests::fixture_window(window, cx);
        let mut snapshot = snapshot();
        snapshot.workspaces[0].label =
            "alpha-with-a-rather-long-workspace-name-that-cannot-fit".into();
        let mut logs = snapshot.tabs[0].clone();
        logs.tab_id = "w1:t2".into();
        logs.number = 2;
        logs.label = "logs".into();
        snapshot.tabs.push(logs);
        view.live.snapshot = Some(Arc::new(snapshot));
        view.endpoints[0].connection.target = sheprd_client::ConnectTarget::Local;
        view
    });
    cx.simulate_resize(size(px(800.), px(600.)));
    cx.update(|window, cx| {
        view.update(cx, |v, cx| {
            v.open_pane_menu("inactive", point(px(100.), px(100.)), window, cx);
            v.activate_pane_menu(Action::MoveTo, window, cx);
            assert_eq!(v.menu.page, Some(Page::MovePane));
        });
        window.draw(cx).clear(cx);
    });
    let row = cx.debug_bounds("pane-move-2").unwrap();
    let place = cx.debug_bounds("pane-move-place-2").unwrap();
    // "logs" is drawn whole, inside its row, rather than squeezed out.
    assert!(place.size.width > px(20.), "{place:?}");
    assert!(place.right() <= row.right(), "{place:?} {row:?}");
}

/// A short workspace name leaves the tab its whole width: the tab's cap is a
/// share of the row, not of the label it sits in.
#[gpui::test]
fn short_workspace_names_leave_the_tab_whole(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(|window, cx| {
        crate::bind_keys(cx);
        let mut view = crate::sidebar::layout_tests::fixture_window(window, cx);
        let mut snapshot = snapshot();
        let mut src = snapshot.workspaces[0].clone();
        src.workspace_id = "w2".into();
        src.label = "x".into();
        snapshot.workspaces.push(src);
        view.live.snapshot = Some(Arc::new(snapshot));
        view.endpoints[0].connection.target = sheprd_client::ConnectTarget::Local;
        view
    });
    cx.simulate_resize(size(px(800.), px(600.)));
    cx.update(|window, cx| {
        view.update(cx, |v, cx| {
            v.open_pane_menu("inactive", point(px(100.), px(100.)), window, cx);
            v.activate_pane_menu(Action::MoveTo, window, cx);
            let places: Vec<_> = v
                .menu
                .pane
                .as_ref()
                .unwrap()
                .target
                .destinations()
                .into_iter()
                .map(|choice| (choice.workspace, choice.place))
                .collect();
            assert_eq!(places[2], (Some("x".into()), "New Tab".into()));
        });
        window.draw(cx).clear(cx);
    });
    // "New Tab" after "x ›" is as wide as "New Tab" on its own.
    let alone = cx.debug_bounds("pane-move-place-0").unwrap();
    let after = cx.debug_bounds("pane-move-place-2").unwrap();
    assert_eq!(after.size.width, alone.size.width, "{after:?} {alone:?}");
}

/// More destinations than the window can show: the keyboard's selection
/// scrolls into view, so Enter never moves the pane somewhere unseen.
#[gpui::test]
fn keyboard_selection_scrolls_into_view(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(|window, cx| {
        crate::bind_keys(cx);
        let mut view = crate::sidebar::layout_tests::fixture_window(window, cx);
        let mut snapshot = snapshot();
        for number in 2..=30 {
            let mut tab = snapshot.tabs[0].clone();
            tab.tab_id = format!("w1:t{number}");
            tab.number = number;
            tab.label = format!("tab {number}");
            snapshot.tabs.push(tab);
        }
        view.live.snapshot = Some(Arc::new(snapshot));
        view.endpoints[0].connection.target = sheprd_client::ConnectTarget::Local;
        view
    });
    cx.simulate_resize(size(px(800.), px(400.)));
    cx.update(|window, cx| {
        view.update(cx, |v, cx| {
            v.open_pane_menu("inactive", point(px(100.), px(100.)), window, cx);
            v.activate_pane_menu(Action::MoveTo, window, cx);
        });
        window.draw(cx).clear(cx);
    });
    // Up from no selection picks the last of 31 destinations.
    cx.simulate_keystrokes("up");
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let last = view.read_with(cx, |v, _| v.menu.pane.as_ref().unwrap().selected.unwrap());
    assert_eq!(last, 30);
    let panel = cx.debug_bounds("menu-panel").unwrap();
    let selector: &'static str = format!("pane-move-{last}").leak();
    let row = cx.debug_bounds(selector).unwrap();
    assert!(
        row.top() >= panel.top() && row.bottom() <= panel.bottom(),
        "{row:?} outside {panel:?}"
    );
}

/// The palette's Move Pane says why a `--dev` window cannot move panes,
/// rather than blaming a custom socket it does not use.
#[gpui::test]
fn development_sessions_explain_why_panes_cannot_move(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(|window, cx| {
        crate::bind_keys(cx);
        let mut view = crate::sidebar::layout_tests::fixture_window(window, cx);
        view.live.snapshot = Some(Arc::new(snapshot()));
        view.endpoints[0].connection.target = sheprd_client::ConnectTarget::Session {
            name: "default".into(),
            development: true,
        };
        view
    });
    cx.update(|window, cx| {
        view.update(cx, |v, cx| {
            v.move_focused_pane(window, cx);
            assert_eq!(v.menu.page, Some(Page::Pane));
            assert_eq!(
                v.menu.pane.as_ref().unwrap().error.as_deref(),
                Some(pane_move::Error::DevelopmentSession.to_string().as_str())
            );
        })
    });
}
