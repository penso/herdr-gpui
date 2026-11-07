use super::*;

/// Clicking a workspace already chose what to do with it, so the hover menu
/// stays closed under the pointer until it leaves the row and comes back.
#[gpui::test]
fn clicking_a_workspace_keeps_its_hover_menu_closed(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(|window, cx| {
        crate::bind_keys(cx);
        let mut view = fixture_window(window, cx);
        view.live.status = crate::state::ConnectionStatus::Connected;
        view.active = true;
        view.config.features.sidebar_hover_menu = true;
        view
    });
    cx.simulate_resize(size(px(900.), px(700.)));
    cx.update(|window, cx| full_draw(window, cx).clear(cx));
    let row = cx.debug_bounds("row-herdr").unwrap().center();
    let settle = |view: &Entity<SheprdWindow>, cx: &mut gpui::VisualTestContext| {
        for elapsed in [
            std::time::Duration::ZERO,
            super::super::HOVER_MENU_DELAY * 2,
        ] {
            cx.update(|window, cx| {
                view.update(cx, |view, cx| {
                    view.poll_hover_menu(std::time::Instant::now() + elapsed, window, cx);
                });
                full_draw(window, cx).clear(cx);
            });
        }
    };

    cx.simulate_mouse_move(row, None, Modifiers::default());
    cx.simulate_mouse_move(row + point(px(8.), px(0.)), None, Modifiers::default());
    cx.simulate_click(row + point(px(8.), px(0.)), Modifiers::default());
    settle(&view, cx);
    assert!(view.read_with(cx, |view, _| view.menu.page.is_none()));

    // Moving within the clicked row still opens nothing.
    cx.simulate_mouse_move(row + point(px(20.), px(0.)), None, Modifiers::default());
    settle(&view, cx);
    assert!(view.read_with(cx, |view, _| view.menu.page.is_none()));

    // Leaving and coming back is a fresh hover, which opens the menu again.
    cx.simulate_mouse_move(point(px(700.), px(400.)), None, Modifiers::default());
    cx.simulate_mouse_move(row, None, Modifiers::default());
    cx.simulate_mouse_move(row + point(px(8.), px(0.)), None, Modifiers::default());
    settle(&view, cx);
    view.read_with(cx, |view, _| {
        assert_eq!(view.menu.page, Some(crate::menu::Page::Workspace));
    });
}
