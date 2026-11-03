use super::*;
use crate::notification_mutes::{Daemon, Mutes};

/// The fixture's workspace `w1` with its tab `w1:t1` and a second tab.
fn snapshot() -> ClientShellSnapshot {
    let mut snapshot: ClientShellSnapshot = serde_json::from_str(include_str!(
        "../../../../herdr-protocol/tests/fixtures/endpoint-snapshot-v1.json"
    ))
    .unwrap();
    let mut tab = snapshot.tabs[0].clone();
    tab.tab_id = "w1:t2".into();
    tab.label = "logs".into();
    tab.focused = false;
    snapshot.tabs.push(tab);
    snapshot
}

fn mute_row(view: &HerdrWindow) -> Action {
    let actions = view.menu.tab.as_ref().unwrap().actions();
    assert_eq!(actions.last(), Some(&Action::Close), "closing stays last");
    let row = actions[actions.len() - 2];
    assert!(matches!(
        row,
        Action::Mute | Action::Unmute | Action::UnmuteWorkspace
    ));
    row
}

#[gpui::test]
fn the_tab_menu_mutes_its_tab_and_lifts_a_workspace_mute_it_falls_under(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(|window, cx| {
        crate::bind_keys(cx);
        let mut view = crate::sidebar::layout_tests::fixture_window(window, cx);
        view.live.snapshot = Some(Arc::new(snapshot()));
        view
    });
    cx.update(|window, cx| {
        view.update(cx, |v, cx| {
            let daemon = Daemon::of(&v.endpoints[0]);
            let snapshot = v.live.snapshot.clone().unwrap();
            let silenced = |cx: &App| {
                let mutes = Mutes::of(cx).unwrap();
                let silenced = mutes.silenced(&daemon, &snapshot);
                (
                    silenced.workspace("w1"),
                    silenced.own_tab("w1:t2"),
                    silenced.tab("w1:t1"),
                )
            };
            v.open_tab_menu("w1:t2", None, Point::default(), window, cx);
            assert_eq!(mute_row(v), Action::Mute);
            assert_eq!(Action::Mute.label(), "Mute Notifications");
            v.activate_tab_menu(Action::Mute, window, cx);
            assert!(v.menu.page.is_none(), "applied at once");
            assert_eq!(silenced(cx), (false, true, false), "only that tab");

            v.open_tab_menu("w1:t2", None, Point::default(), window, cx);
            assert_eq!(mute_row(v), Action::Unmute);
            v.activate_tab_menu(Action::Unmute, window, cx);
            assert_eq!(silenced(cx), (false, false, false));

            Mutes::update(cx, |mutes| {
                mutes.set(
                    &daemon,
                    &snapshot,
                    Scope::Workspace,
                    "w1",
                    true,
                    std::time::SystemTime::now(),
                )
            });
            v.open_tab_menu("w1:t1", None, Point::default(), window, cx);
            assert_eq!(mute_row(v), Action::UnmuteWorkspace);
            v.activate_tab_menu(Action::UnmuteWorkspace, window, cx);
            assert_eq!(silenced(cx), (false, false, false));
        })
    });
}

#[gpui::test]
fn a_mute_row_from_a_stale_menu_changes_nothing(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(|window, cx| {
        crate::bind_keys(cx);
        let mut view = crate::sidebar::layout_tests::fixture_window(window, cx);
        view.live.snapshot = Some(Arc::new(snapshot()));
        view
    });
    cx.update(|window, cx| {
        view.update(cx, |v, cx| {
            let daemon = Daemon::of(&v.endpoints[0]);
            for stale in [false, true] {
                v.open_tab_menu("w1:t2", None, Point::default(), window, cx);
                if stale {
                    v.selection_epoch += 1;
                } else {
                    // A reconnection to another daemon process.
                    let snapshot = Arc::make_mut(v.live.snapshot.as_mut().unwrap());
                    snapshot.boot_id.push_str("-restarted");
                }
                v.activate_tab_menu(Action::Mute, window, cx);
                let snapshot = v.live.snapshot.clone().unwrap();
                assert!(
                    Mutes::of(cx).is_none_or(|mutes| mutes.silenced(&daemon, &snapshot).is_empty())
                );
                v.dismiss_menu(window, cx);
            }
        })
    });
}
