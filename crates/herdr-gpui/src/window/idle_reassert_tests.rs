//! A window nobody is using leaves a frame another client sized alone, so
//! two windows of different sizes on one tab do not take it from each other.

use super::HerdrWindow;
use super::lifecycle::{REASSERT_AFTER_INPUT, RESIZE_REASSERT, RESIZE_SETTLE};
use herdr_client::ConnectTarget;
use herdr_client::protocol::{FrameData, PaneSurfaceFrame};
use std::sync::Arc;
use std::time::Instant;

/// A window whose queued size the daemon answered with a frame one column
/// wider, as when another client resized the tab. Its handle is
/// disconnected, so any send shows up as a `Resize:` error.
fn overridden_window(
    cx: &mut gpui::TestAppContext,
) -> (gpui::Entity<HerdrWindow>, &mut gpui::VisualTestContext) {
    let (view, cx) = cx.add_window_view(|window, cx| {
        HerdrWindow::new(
            ConnectTarget::Socket("/unused-idle-reassert-test.sock".into()),
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
    });
    (view, cx)
}

#[gpui::test]
fn an_idle_window_leaves_a_frame_another_client_sized(cx: &mut gpui::TestAppContext) {
    let (view, cx) = overridden_window(cx);
    view.update(cx, |view, _| {
        assert!(view.last_input.is_none(), "a new window has had no input");
        let start = Instant::now();
        for at in [
            start + RESIZE_SETTLE,
            start + RESIZE_REASSERT + RESIZE_SETTLE,
            start + RESIZE_REASSERT * 3,
        ] {
            view.resize_at(at);
            assert!(
                view.local_error.is_none(),
                "an idle window does not re-claim the frame: {:?}",
                view.local_error
            );
        }
    });
}

#[gpui::test]
fn a_window_whose_input_is_old_leaves_a_frame_another_client_sized(cx: &mut gpui::TestAppContext) {
    let (view, cx) = overridden_window(cx);
    view.update(cx, |view, _| {
        let start = Instant::now();
        view.last_input = Some(start);
        let later = start + REASSERT_AFTER_INPUT;
        for at in [
            later,
            later + RESIZE_REASSERT + RESIZE_SETTLE,
            later + RESIZE_REASSERT * 3,
        ] {
            view.resize_at(at);
            assert!(
                view.local_error.is_none(),
                "input {REASSERT_AFTER_INPUT:?} old no longer re-claims the frame: {:?}",
                view.local_error
            );
        }
    });
}

#[gpui::test]
fn an_idle_window_still_sends_its_own_new_size(cx: &mut gpui::TestAppContext) {
    let (view, cx) = overridden_window(cx);
    view.update(cx, |view, _| {
        let start = Instant::now();
        view.options.surface_size.cols += 1;
        view.resize_at(start);
        assert!(
            view.local_error.is_none(),
            "a new size still settles before a send"
        );
        view.resize_at(start + RESIZE_SETTLE);
        assert!(
            view.local_error.is_some(),
            "an idle window still sends a size of its own once settled"
        );
    });
}

#[gpui::test]
fn a_pointer_that_did_not_move_is_not_use(cx: &mut gpui::TestAppContext) {
    let (view, cx) = overridden_window(cx);
    view.update(cx, |view, _| {
        let resting = gpui::point(gpui::px(40.), gpui::px(30.));
        view.note_pointer(resting);
        assert!(view.last_input.is_some(), "pointing at a pane is use");
        view.last_input = None;
        view.note_pointer(resting);
        assert!(
            view.last_input.is_none(),
            "a move event for a cursor that did not move is not use"
        );
        view.note_pointer(resting + gpui::point(gpui::px(1.), gpui::px(0.)));
        assert!(view.last_input.is_some(), "moving the pointer again is use");
    });
}

/// Whether a window that just had input re-claims the frame another client
/// sized: its disconnected handle turns the resize into a local error.
fn reclaims(view: &mut HerdrWindow) -> bool {
    let now = Instant::now();
    view.resize_at(now);
    view.resize_at(now + RESIZE_REASSERT + RESIZE_SETTLE);
    view.local_error.is_some()
}

#[gpui::test]
fn a_wheel_on_a_frame_another_client_sized_is_use(cx: &mut gpui::TestAppContext) {
    let (view, cx) = overridden_window(cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            assert!(!view.input_ready(), "the fixture holds input");
            let wheel = gpui::ScrollWheelEvent {
                position: gpui::point(gpui::px(15.), gpui::px(10.)),
                ..Default::default()
            };
            view.scroll_wheel(&wheel, window, cx);
            assert!(view.last_input.is_some(), "a wheel over the panes is use");
            assert!(reclaims(view), "the wheel re-claims the frame");
        })
    });
}

#[gpui::test]
fn a_stationary_click_on_a_frame_another_client_sized_is_use(cx: &mut gpui::TestAppContext) {
    let (view, cx) = overridden_window(cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            let resting = gpui::point(gpui::px(15.), gpui::px(10.));
            view.last_pointer = Some(resting);
            let press = gpui::MouseDownEvent {
                button: gpui::MouseButton::Left,
                position: resting,
                modifiers: gpui::Modifiers::default(),
                click_count: 1,
                first_mouse: false,
            };
            view.terminal_mouse_down(&press, window, cx);
            assert!(view.last_input.is_some(), "a press over the panes is use");
            assert!(reclaims(view), "the press re-claims the frame");
        })
    });
}

#[gpui::test]
fn a_paste_on_a_frame_another_client_sized_is_use(cx: &mut gpui::TestAppContext) {
    let (view, cx) = overridden_window(cx);
    view.update(cx, |view, cx| {
        view.live.status = crate::state::ConnectionStatus::Connected;
        assert!(!view.input_ready(), "the fixture holds input");
        cx.write_to_clipboard(gpui::ClipboardItem::new_string("x".into()));
        view.paste(cx);
        assert!(
            view.local_error.is_none(),
            "a paste waits for a ready frame instead of being sent: {:?}",
            view.local_error
        );
        assert!(view.last_input.is_some(), "a paste is use");
        assert!(reclaims(view), "the paste re-claims the frame");
    });
}

#[gpui::test]
fn a_drop_on_a_frame_another_client_sized_is_use(cx: &mut gpui::TestAppContext) {
    let (view, cx) = overridden_window(cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            assert!(!view.input_ready(), "the fixture holds input");
            view.bounds = gpui::Bounds::new(
                gpui::point(gpui::px(0.), gpui::px(0.)),
                gpui::size(gpui::px(800.), gpui::px(600.)),
            );
            assert!(view.bounds.contains(&window.mouse_position()));
            view.drop_terminal_files(&gpui::ExternalPaths::default(), window, cx);
            assert!(view.last_input.is_some(), "a drop over the panes is use");
            assert!(reclaims(view), "the drop re-claims the frame");
        })
    });
}

#[gpui::test]
fn a_drop_outside_the_panes_is_not_use(cx: &mut gpui::TestAppContext) {
    let (view, cx) = overridden_window(cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.bounds = gpui::Bounds::new(
                gpui::point(gpui::px(1000.), gpui::px(1000.)),
                gpui::size(gpui::px(10.), gpui::px(10.)),
            );
            assert!(!view.bounds.contains(&window.mouse_position()));
            view.drop_terminal_files(&gpui::ExternalPaths::default(), window, cx);
            assert!(view.last_input.is_none(), "a drop on the chrome is not use");
        })
    });
}

#[gpui::test]
fn input_held_across_a_projection_gap_counts_as_use(cx: &mut gpui::TestAppContext) {
    let (view, cx) = overridden_window(cx);
    view.update(cx, |view, cx| {
        // The frame disagrees with the window's size, so input is not ready
        // and is held rather than sent; the connection itself is up.
        view.live.status = crate::state::ConnectionStatus::Connected;
        assert!(!view.input_ready(), "the fixture holds input");
        view.send(
            herdr_client::protocol::ClientPaneInputEvent::Paste("x".into()),
            cx,
        );
        assert!(
            view.last_input.is_some(),
            "held input marks the window as used"
        );
    });
}
