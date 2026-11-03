use super::*;
use crate::pane_move::{self, Destination};

fn snapshot() -> ClientShellSnapshot {
    let mut snapshot: ClientShellSnapshot = serde_json::from_str(include_str!(
        "../../../../sheprd-protocol/tests/fixtures/endpoint-snapshot-v1.json"
    ))
    .unwrap();
    let mut tab = snapshot.tabs[0].clone();
    tab.tab_id = "inactive".into();
    tab.number = 2;
    tab.label = "logs".into();
    tab.focused = false;
    snapshot.tabs.push(tab);
    let mut pane1 = snapshot.panes[0].clone();
    pane1.pane_id = "inactive:p1".into();
    pane1.tab_id = "inactive".into();
    pane1.focused = false;
    let mut pane2 = pane1.clone();
    pane2.pane_id = "inactive:p2".into();
    snapshot.panes.push(pane1);
    snapshot.panes.push(pane2);
    let mut workspace = snapshot.workspaces[0].clone();
    workspace.workspace_id = "w2".into();
    workspace.number = 2;
    workspace.label = "other".into();
    workspace.focused = false;
    snapshot.workspaces.push(workspace);
    snapshot
}

#[gpui::test]
fn tab_moves_offer_workspaces_and_settle_once(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(|window, cx| {
        crate::bind_keys(cx);
        let mut view = crate::sidebar::layout_tests::fixture_window(window, cx);
        view.live.snapshot = Some(Arc::new(snapshot()));
        view
    });
    cx.simulate_resize(size(px(800.), px(600.)));
    cx.update(|window, cx| {
        view.update(cx, |v, cx| {
            // The fixture's custom socket has no host for the CLI.
            v.open_tab_menu("inactive", None, Point::default(), window, cx);
            let tab = v.menu.tab.as_ref().unwrap();
            assert!(!tab.movable);
            assert!(!tab.actions().contains(&Action::MoveToWorkspace));
            v.dismiss_menu(window, cx);

            v.endpoints[0].connection.target = sheprd_client::ConnectTarget::Local;
            v.open_tab_menu("inactive", None, Point::default(), window, cx);
            let tab = v.menu.tab.as_ref().unwrap();
            assert!(tab.actions().contains(&Action::MoveToWorkspace));
            assert_eq!(tab.target.panes, ["inactive:p1", "inactive:p2"]);
            v.activate_tab_menu(Action::MoveToWorkspace, window, cx);
            assert_eq!(v.menu.page, Some(Page::MoveTab));
            let places: Vec<_> = v
                .menu
                .tab
                .as_ref()
                .unwrap()
                .target
                .choices
                .iter()
                .map(|choice| choice.place.as_str())
                .collect();
            assert_eq!(places, ["New Workspace", "other"]);
        });
        window.draw(cx).clear(cx);
    });
    assert!(cx.debug_bounds("tab-move-0").is_some());
    assert!(cx.debug_bounds("tab-move-1").is_some());
    assert!(cx.debug_bounds("tab-move-2").is_none());
    cx.simulate_keystrokes("down down up");
    view.read_with(cx, |v, _| {
        assert_eq!(v.menu.tab.as_ref().unwrap().selected, Some(0));
    });
    cx.update(|window, cx| {
        view.update(cx, |v, cx| {
            // Destination workspace closed after the picker opened: nothing runs.
            Arc::make_mut(v.live.snapshot.as_mut().unwrap())
                .workspaces
                .retain(|workspace| workspace.workspace_id != "w2");
            v.move_tab(
                Destination::NewTab {
                    workspace_id: "w2".into(),
                },
                window,
                cx,
            );
            let tab = v.menu.tab.as_ref().unwrap();
            assert!(tab.moving.is_none());
            assert_eq!(
                tab.error.as_deref(),
                Some(crate::Error::StaleWorkspace.to_string().as_str())
            );

            // A pane in the tab closed after the menu opened: refused as stale.
            Arc::make_mut(v.live.snapshot.as_mut().unwrap())
                .panes
                .retain(|pane| pane.pane_id != "inactive:p2");
            v.move_tab(Destination::NewWorkspace, window, cx);
            let tab = v.menu.tab.as_ref().unwrap();
            assert!(tab.moving.is_none());
            assert_eq!(
                tab.error.as_deref(),
                Some(crate::Error::StaleTab.to_string().as_str())
            );

            // While a move runs, the menu starts no other.
            v.menu.tab.as_mut().unwrap().moving = Some(7);
            v.activate_tab_menu(Action::MoveToWorkspace, window, cx);
            v.move_tab(Destination::NewWorkspace, window, cx);
            assert_eq!(v.menu.tab.as_ref().unwrap().moving, Some(7));
            // Another move's late failure reaches the window, not this menu.
            v.local_error = None;
            v.finish_tab_move(
                6,
                Err(pane_move::Error::Unchanged(pane_move::Unchanged::SameTab)),
                window,
                cx,
            );
            assert_eq!(v.menu.tab.as_ref().unwrap().moving, Some(7));
            assert_eq!(
                v.local_error.as_deref(),
                Some("The pane is already in that tab.")
            );
            v.finish_tab_move(
                7,
                Err(pane_move::Error::Unchanged(pane_move::Unchanged::ZoomedTab)),
                window,
                cx,
            );
            let tab = v.menu.tab.as_ref().unwrap();
            assert!(tab.moving.is_none());
            assert!(tab.error.as_ref().unwrap().contains("Unzoom"));
            assert_eq!(v.menu.page, Some(Page::MoveTab));
            v.menu.tab.as_mut().unwrap().moving = Some(8);
            v.finish_tab_move(8, Ok(pane_move::TabMoved::default()), window, cx);
            assert!(v.menu.page.is_none());
            // A success after the menu closed changes nothing visible.
            v.local_error = None;
            v.finish_tab_move(9, Ok(pane_move::TabMoved::default()), window, cx);
            assert!(v.local_error.is_none());
            // A move that lost the tab's name closes the menu and says so.
            v.open_tab_menu("inactive", None, Point::default(), window, cx);
            v.menu.tab.as_mut().unwrap().moving = Some(10);
            let moved = pane_move::TabMoved {
                warning: Some(pane_move::Error::Rename {
                    tab_id: "w2:t9".into(),
                    source: Box::new(pane_move::Error::Unchanged(pane_move::Unchanged::Other)),
                }),
            };
            v.finish_tab_move(10, Ok(moved), window, cx);
            assert!(v.menu.tab.is_none());
            assert!(
                v.local_error
                    .as_deref()
                    .unwrap()
                    .starts_with("Moved the tab, but could not keep its name")
            );
        })
    });
}

#[gpui::test]
fn double_click_into_the_tab_move_picker_moves_nothing(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(|window, cx| {
        crate::bind_keys(cx);
        let mut view = crate::sidebar::layout_tests::fixture_window(window, cx);
        view.live.snapshot = Some(Arc::new(snapshot()));
        view.endpoints[0].connection.target = sheprd_client::ConnectTarget::Session {
            name: "move-test-no-such-session".into(),
            development: false,
        };
        view
    });
    cx.simulate_resize(size(px(800.), px(600.)));
    let index = cx.update(|window, cx| {
        view.update(cx, |v, cx| {
            v.open_tab_menu("inactive", None, point(px(100.), px(100.)), window, cx);
            v.menu
                .tab
                .as_ref()
                .unwrap()
                .actions()
                .iter()
                .position(|action| matches!(action, Action::MoveToWorkspace))
                .unwrap()
        })
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let selector: &'static str = format!("tab-menu-{index}").leak();
    let row = cx.debug_bounds(selector).unwrap().center();
    cx.simulate_click(row, Modifiers::default());
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let picked = cx.debug_bounds("tab-move-0").unwrap().center();
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
        assert_eq!(v.menu.page, Some(Page::MoveTab));
        let tab = v.menu.tab.as_ref().unwrap();
        assert!(tab.moving.is_none() && tab.error.is_none());
    });
    cx.update(|_, cx| {
        view.update(cx, |v, _| {
            Arc::make_mut(v.live.snapshot.as_mut().unwrap()).boot_id = "restarted".into();
        })
    });
    cx.simulate_click(picked, Modifiers::default());
    view.read_with(cx, |v, _| {
        let tab = v.menu.tab.as_ref().unwrap();
        assert!(tab.moving.is_none());
        assert_eq!(
            tab.error.as_deref(),
            Some(crate::Error::StaleTab.to_string().as_str())
        );
    });
}
