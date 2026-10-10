use super::*;

#[gpui::test]
fn local_peer_warning_shows_recovery_text_without_stealing_focus(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(fixture_window);
    for width in [1000., 360.] {
        cx.simulate_resize(size(px(width), px(600.)));
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                window.focus(&view.focus, cx);
                view.config.notifications.enabled = false;
                view.endpoints[0].toasts = Default::default();
                let warning = crate::daemon::LocalPeerWarning::new(
                    crate::daemon::UntrustedEndpoint::DirectoryPermissions,
                    "/home/test/.config/herdr/herdr-client.sock".into(),
                );
                view.endpoints[0]
                    .toasts
                    .receive([warning.notice(Instant::now())]);
            });
            window.draw(cx).clear(cx);
            assert!(view.read(cx).focus.is_focused(window));
            assert!(view.read(cx).menu.page.is_none());
        });
        let card = cx.debug_bounds("toast-local-0").unwrap();
        let body = cx.debug_bounds("toast-body-local-0").unwrap();
        assert!(
            body.size.height > px(48.),
            "recovery text must not be a two-line preview"
        );
        assert!(body.bottom() <= card.bottom());
        assert!(card.right() <= px(width));
        let dismiss = cx.debug_bounds("toast-dismiss-local-0").unwrap();
        cx.simulate_click(dismiss.center(), Default::default());
        assert!(view.read_with(cx, |view, _| view.endpoints[0].toasts.entries.is_empty()));
    }
}
