//! Lanes are created through the origin workspace, whose repository Herdr
//! keeps while its shell moves. A main checkout whose shell left it offers no
//! fan-out, and one that leaves while the prompt is open launches nothing.

use super::*;

fn connected(view: &mut HerdrWindow) {
    view.live.status = crate::state::ConnectionStatus::Connected;
    view.endpoints[0].connection.target = herdr_client::ConnectTarget::Local;
    let snapshot = std::sync::Arc::make_mut(view.live.snapshot.as_mut().unwrap());
    snapshot.workspaces = crate::sidebar::layout_tests::snapshot(7).workspaces;
}

fn move_main_checkout(view: &mut HerdrWindow) {
    let snapshot = std::sync::Arc::make_mut(view.live.snapshot.as_mut().unwrap());
    snapshot.workspaces[3].new_workspace_cwd = "/fixture/wgsextract-cli".into();
}

#[gpui::test]
fn a_moved_main_checkout_offers_no_fan_out(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(crate::sidebar::layout_tests::fixture_window);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            connected(view);
            move_main_checkout(view);
            // The linked checkout's only main checkout has wandered off too.
            for workspace in ["w3", "w4"] {
                view.open_workspace_menu(workspace, Default::default(), window, cx);
                assert_eq!(offered(view), None, "{workspace}");
                view.dismiss_menu(window, cx);
            }
        })
    });
}

#[gpui::test]
fn a_main_checkout_that_moves_under_the_prompt_launches_nothing(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(crate::sidebar::layout_tests::fixture_window);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            connected(view);
            view.open_workspace_menu("w3", Default::default(), window, cx);
            view.activate_workspace_menu(WorkspaceMenuAction::FanOut, window, cx);
            let fan_out = view.fan_out.as_mut().unwrap();
            fan_out.agents_for_test(vec![crate::teleport::AgentKind::Claude]);
            assert!(view.fan_out_origin_current());
            move_main_checkout(view);
            assert!(!view.fan_out_origin_current());
            view.menu.input = Some(DialogInput::new("Fix the bug".into()));
        })
    });
    cx.simulate_keystrokes("enter");
    cx.update(|_, cx| {
        let fan_out = view.read(cx).fan_out.as_ref().unwrap();
        assert!(fan_out.composing(), "nothing was launched");
        assert!(
            fan_out
                .error_for_test()
                .is_some_and(|e| e.contains("moved"))
        );
    });
}
