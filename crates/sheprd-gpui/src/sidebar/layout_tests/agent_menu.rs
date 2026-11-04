use super::*;
use crate::{menu::Page, state::ConnectionStatus};
use gpui::MouseButton;

/// Right-clicking an agent's row opens its pane's menu, on whichever host
/// the agent runs: the window switches to that host and shows the pane
/// first, as a workspace row's menu does.
#[gpui::test]
fn agent_rows_open_their_panes_menu_on_any_host(cx: &mut gpui::TestAppContext) {
    let (fixture, cx) = cx.add_window_view(|window, cx| {
        let view = cx.new(|cx| {
            let mut view = fixture_window(window, cx);
            let mut listing = snapshot(1);
            listing.agents.truncate(1);
            listing.panes = vec![
                serde_json::from_value(serde_json::json!({
                    "pane_id": "p0", "workspace_id": "w0", "tab_id": "t0", "label": "review",
                    "cwd": "/repo", "foreground_cwd": "/repo", "focused": false,
                    "right_click_passthrough": false,
                }))
                .unwrap(),
            ];
            view.live.snapshot = Some(Arc::new(listing.clone()));
            view.live.status = ConnectionStatus::Connected;
            let mut remote = crate::endpoint::Endpoint::new(
                "ssh:test".into(),
                "Remote".into(),
                ConnectTarget::Ssh {
                    target: "unused".into(),
                    session: "default".into(),
                },
                true,
            );
            remote.live.snapshot = Some(Arc::new(listing));
            remote.live.status = ConnectionStatus::Connected;
            view.endpoints.push(remote);
            view
        });
        cx.observe(&view, |_, _, cx| cx.notify()).detach();
        SidebarFixture(view)
    });
    let view = cx.update(|_, cx| fixture.read(cx).0.clone());
    cx.simulate_resize(size(px(800.), px(600.)));
    cx.run_until_parked();
    for (row, host) in [("agent-local-p0", 0), ("agent-ssh:test-p0", 1)] {
        cx.update(|window, cx| full_draw(window, cx).clear(cx));
        let position = cx
            .debug_bounds(row)
            .unwrap_or_else(|| panic!("{row} not painted"))
            .center();
        cx.simulate_mouse_down(position, MouseButton::Right, Modifiers::default());
        cx.update(|window, cx| {
            full_draw(window, cx).clear(cx);
            let view = view.read(cx);
            assert_eq!(view.selected_endpoint, host, "{row}");
            assert_eq!(view.menu.page, Some(Page::Pane), "{row}");
            assert_eq!(view.pane_menu_target(), Some("p0"), "{row}");
            assert!(view.menu.opening_right_click, "{row}");
            assert!(view.menu.focus.is_focused(window), "{row}");
        });
        cx.simulate_mouse_up(position, MouseButton::Right, Modifiers::default());
        cx.update(|window, cx| view.update(cx, |view, cx| view.dismiss_menu(window, cx)));
    }
}
