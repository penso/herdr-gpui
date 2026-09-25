use super::HerdrWindow;
use herdr_client::ConnectTarget;

#[gpui_kit::test]
fn resize_tracks_cell_metrics_and_retries_failed_options(cx: &mut gpui_kit::TestAppContext) {
    let (view, cx) = crate::test_support::add_window_view(cx, |window, cx| {
        HerdrWindow::new(
            ConnectTarget::Socket("/unused-resize-test.sock".into()),
            window,
            cx,
            true,
        )
    });
    view.update(cx, |view, _| {
        let client = herdr_client::connect(
            view.endpoints[view.selected_endpoint]
                .connection
                .target
                .clone(),
            view.options,
        )
        .unwrap_or_else(|error| panic!("cannot create test client: {error}"));
        client.handle.disconnect();
        view.endpoints[view.selected_endpoint].connection.handle = Some(client.handle);
        let queued = view.options;
        view.last_queued_options = Some(queued);
        view.resize();
        assert!(
            view.local_error.is_none(),
            "identical options are not resent"
        );
        let start = std::time::Instant::now();
        view.options.surface_size.cols += 1;
        view.resize_at(start);
        view.options.surface_size.cols += 1;
        view.resize_at(start + super::lifecycle::RESIZE_SETTLE);
        assert!(
            view.local_error.is_none(),
            "a size still changing is not sent"
        );
        let settled = start + super::lifecycle::RESIZE_SETTLE * 2;
        view.options.cell_width_px += 1;
        view.resize_at(settled);
        view.resize_at(settled + super::lifecycle::RESIZE_SETTLE / 2);
        assert!(
            view.local_error.is_none(),
            "the last change restarts the wait"
        );
        view.resize_at(settled + super::lifecycle::RESIZE_SETTLE);
        assert!(
            view.local_error.is_some(),
            "cell metrics alone trigger a send once settled"
        );
        assert_eq!(view.last_queued_options, Some(queued));
        view.local_error = None;
        view.resize_at(settled + super::lifecycle::RESIZE_SETTLE);
        assert!(view.local_error.is_some(), "failed options are retried");
    });
}
