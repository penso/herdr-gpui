use super::*;

#[gpui::test]
fn saved_warning_stays_until_dismissed_and_wraps_in_a_narrow_window(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(fixture_window);
    for width in [1000., 360.] {
        cx.simulate_resize(gpui::size(gpui::px(width), gpui::px(600.)));
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                let request = setup::Request::new("remote.test", "Remote", "").unwrap();
                open_form(view, Step::Saving(request, HostProbe::Running), cx);
                view.device_save_succeeded(setup::SaveOutcome::ExistingServer, cx);
                assert!(view.menu.device_setup.as_ref().unwrap().error.is_none());
                assert!(view.flash.is_none());
                view.tick_flash(std::time::Instant::now() + std::time::Duration::from_secs(60));
                view.poll_device_setup(window, cx);
                assert!(matches!(step(view), Step::SavedWarning(text)
                    if Some(*text) == setup::SaveOutcome::ExistingServer.warning()));
            });
            window.draw(cx).clear(cx);
        });
        let warning = cx.debug_bounds("device-saved-warning").unwrap();
        let body = cx.debug_bounds("device-saved-warning-scroll").unwrap();
        let done = cx.debug_bounds("device-saved-done").unwrap();
        assert!(
            warning.size.height > gpui::px(32.),
            "full recovery advice must wrap"
        );
        assert!(warning.right() <= gpui::px(width));
        assert!(warning.origin.x >= gpui::px(0.));
        assert!(warning.bottom() <= body.bottom());
        assert!(done.origin.y >= body.bottom());
        cx.simulate_click(done.center(), Default::default());
        cx.update(|window, cx| {
            assert!(view.read(cx).menu.device_setup.is_none());
            assert!(view.read(cx).menu.page.is_none());
            assert!(view.read(cx).focus.is_focused(window));
        });
    }
}

#[gpui::test]
fn saved_warning_dismisses_with_enter_or_escape(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(fixture_window);
    for key in ["enter", "escape"] {
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                open_form(view, Step::Saved, cx);
                let input = view.menu.device_setup.as_ref().unwrap().fields[0]
                    .read(cx)
                    .focus
                    .clone();
                window.focus(&input, cx);
                view.device_save_succeeded(setup::SaveOutcome::ExistingServer, cx);
                view.poll_device_setup(window, cx);
                assert!(view.menu.focus.is_focused(window));
            });
            window.draw(cx).clear(cx);
        });
        cx.simulate_keystrokes(key);
        cx.update(|window, cx| {
            assert!(view.read(cx).menu.device_setup.is_none());
            assert!(view.read(cx).focus.is_focused(window));
        });
    }
}

#[gpui::test]
fn ordinary_save_still_closes_without_an_acknowledgement(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(fixture_window);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            open_form(view, Step::Saved, cx);
            view.device_save_succeeded(setup::SaveOutcome::Ready, cx);
            view.poll_device_setup(window, cx);
            assert!(view.menu.device_setup.is_none());
        });
    });
}
