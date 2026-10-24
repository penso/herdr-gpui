use super::HerdrWindow;
use herdr_client::ConnectTarget;
use herdr_client::protocol::{FrameData, PaneSurfaceFrame};
use std::sync::Arc;

#[gpui::test]
fn resize_tracks_cell_metrics_and_retries_failed_options(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(|window, cx| {
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
        view.endpoints[view.selected_endpoint].attach_shown(client.handle);
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

#[gpui::test]
fn resize_reasks_when_the_daemon_frame_is_a_different_size(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(|window, cx| {
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
        view.endpoints[view.selected_endpoint].attach_shown(client.handle);
        // The window already asked for its size, and the daemon projects a
        // frame of another size: another client resized the tab.
        let queued = view.options;
        view.last_queued_options = Some(queued);
        view.live.surface = Some(Arc::new(PaneSurfaceFrame {
            boot_id: String::new(),
            projection_revision: 0,
            surface_revision: 0,
            frame: FrameData {
                width: queued.surface_size.cols.saturating_add(1),
                height: queued.surface_size.rows,
                cells: Vec::new(),
                cursor: None,
                hyperlinks: Vec::new(),
                graphics: Vec::new(),
            },
            panes: Vec::new(),
            splits: Vec::new(),
            popup: None,
            graphics: Default::default(),
        }));
        let start = std::time::Instant::now();
        // The window is in use, so it keeps the tab at its own size.
        view.last_input = Some(start);
        view.resize_at(start + super::lifecycle::RESIZE_SETTLE);
        assert!(
            view.local_error.is_none(),
            "a disagreeing frame still settles before a send"
        );
        view.resize_at(start + super::lifecycle::RESIZE_REASSERT);
        assert!(
            view.local_error.is_none(),
            "a frame of another size is not re-claimed before it has been answered"
        );
        view.resize_at(start + super::lifecycle::RESIZE_REASSERT + super::lifecycle::RESIZE_SETTLE);
        assert!(
            view.local_error.is_some(),
            "a frame of another size asks for the size again"
        );
    });
}

#[gpui::test]
fn a_full_command_queue_is_retried_without_a_status_error(cx: &mut gpui::TestAppContext) {
    use herdr_client::Stream;
    use std::time::{Duration, Instant};

    let (view, cx) = cx.add_window_view(|window, cx| {
        HerdrWindow::new(
            ConnectTarget::Socket("/unused-resize-test.sock".into()),
            window,
            cx,
            true,
        )
    });
    view.update(cx, |view, _| {
        // Nothing drains the queue until the gate opens and the worker connects.
        let (stream, _server) =
            Stream::pair().unwrap_or_else(|error| panic!("cannot create stream pair: {error}"));
        let (open, gate) = std::sync::mpsc::channel::<()>();
        let client = herdr_client::connect_with_connector(
            view.endpoints[view.selected_endpoint]
                .connection
                .target
                .clone(),
            view.options,
            true,
            move |_, _| {
                let _ = gate.recv();
                Ok(stream)
            },
        )
        .unwrap_or_else(|error| panic!("cannot create test client: {error}"));
        let boot = view
            .live
            .snapshot
            .as_ref()
            .map(|snapshot| snapshot.boot_id.clone())
            .unwrap_or_else(|| panic!("the test window has a snapshot"));
        while client.handle.set_focus(&boot, true).is_ok() {}
        assert!(matches!(
            client.handle.set_focus(&boot, true),
            Err(herdr_client::Error::Full)
        ));
        view.endpoints[view.selected_endpoint].attach_shown(client.handle.clone());
        let queued = view.options;
        view.last_queued_options = Some(queued);
        let start = Instant::now();
        view.options.surface_size.cols += 1;
        view.resize_at(start);
        view.resize_at(start + super::lifecycle::RESIZE_SETTLE);
        assert!(
            view.local_error.is_none(),
            "backpressure is not reported: {:?}",
            view.local_error
        );
        assert_eq!(
            view.last_queued_options,
            Some(queued),
            "unqueued options stay pending for the next retry"
        );

        // The worker connects and drains the backlog (no welcome yet, so it
        // rejects each command); the next resize then goes out.
        open.send(())
            .unwrap_or_else(|error| panic!("the connector is gone: {error}"));
        let deadline = Instant::now() + Duration::from_secs(5);
        while view.last_queued_options != Some(view.options) {
            assert!(Instant::now() < deadline, "the resize was never retried");
            while client.events.try_recv().is_ok() {}
            std::thread::sleep(Duration::from_millis(1));
            view.resize_at(start + super::lifecycle::RESIZE_SETTLE);
        }
        assert!(
            view.local_error.is_none(),
            "a retried resize leaves no status error: {:?}",
            view.local_error
        );
        client.handle.disconnect();
    });
}
