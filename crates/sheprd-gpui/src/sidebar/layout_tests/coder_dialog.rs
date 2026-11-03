use super::*;

#[gpui::test]
fn the_coder_pane_of_add_device_fits(cx: &mut gpui::TestAppContext) {
    let (fixture, cx) = cx.add_window_view(|window, cx| {
        crate::bind_keys(cx);
        let view = cx.new(|cx| fixture_window(window, cx));
        cx.observe(&view, |_, _, cx| cx.notify()).detach();
        SidebarFixture(view)
    });
    let view = cx.update(|_, cx| fixture.read(cx).0.clone());
    cx.simulate_resize(size(px(800.), px(600.)));
    cx.run_until_parked();
    cx.update(|window, cx| full_draw(window, cx).clear(cx));
    if crate::cloud::unavailable().is_some() {
        return;
    }
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.open_coder_fixture(window, cx));
        full_draw(window, cx).clear(cx);
    });
    for (width, height) in [(320., 300.), (800., 600.)] {
        cx.simulate_resize(size(px(width), px(height)));
        cx.update(|window, cx| full_draw(window, cx).clear(cx));
        let panel = cx.debug_bounds("menu-panel").unwrap();
        let dialog = cx.debug_bounds("coder-setup-dialog").unwrap();
        let submit = cx.debug_bounds("coder-setup-submit").unwrap();
        assert!(dialog.right() <= panel.right() && dialog.left() >= panel.left());
        assert!(submit.bottom() <= panel.bottom());
        assert!(panel.bottom() <= px(height));
        assert!(panel.right() <= px(width));
        // A narrow window keeps the rail to icons so the form has room.
        let rail = cx.debug_bounds("add-device-rail").unwrap();
        assert!(rail.left() >= dialog.left() && rail.bottom() <= panel.bottom());
        if width < 400. {
            assert!(rail.size.width < px(60.), "{rail:?}");
        } else {
            assert!(rail.size.width > px(150.), "{rail:?}");
        }
    }
    // Typing stays in the dialog; shortcuts cannot reach the terminal behind it.
    cx.simulate_keystrokes("cmd-b");
    cx.update(|_, cx| assert!(view.read(cx).sidebar_visible));
    cx.simulate_keystrokes("escape");
    cx.update(|window, cx| {
        assert!(view.read(cx).menu.page.is_none());
        assert!(view.read(cx).focus.is_focused(window));
    });
}
