#![allow(clippy::unwrap_used)]

use crate::{
    WINDOW_TITLE,
    controls::Command,
    sidebar::layout_tests::{fixture_window, snapshot},
};
use std::sync::Arc;

fn main_windows(cx: &mut gpui_kit::App) -> Vec<crate::app::MainWindow> {
    crate::app::MainWindow::all(cx)
}

#[gpui_kit::test]
fn new_window_adds_one_client_of_the_same_target_without_disturbing_the_first(
    cx: &mut gpui_kit::TestAppContext,
) {
    let (view, cx) = crate::test_support::add_window_view(cx, fixture_window);
    let target = view.update(cx, |view, _| view.endpoints[0].connection.target.clone());
    let before = cx.cx.update(main_windows);
    assert_eq!(before.len(), 1);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.command(Command::NewWindow, window, cx));
    });
    cx.run_until_parked();
    let opened = cx.cx.update(main_windows);
    assert_eq!(opened.len(), 2, "one more window onto the same daemon");
    let second = opened
        .into_iter()
        .find(|handle| !before.contains(handle))
        .unwrap();
    let first_inbox = view.update(cx, |view, _| view.endpoints[0].connection.inbox.clone());
    cx.update(|_, cx| {
        // The new window is a separate client: its own endpoint and inbox.
        second
            .update(cx, |second, _, _| {
                assert_eq!(second.endpoints.len(), 1);
                assert_eq!(second.endpoints[0].connection.target, target);
                assert!(!Arc::ptr_eq(
                    &second.endpoints[0].connection.inbox,
                    &first_inbox
                ));
            })
            .unwrap();
    });
    // The originating window keeps its own selection and error state.
    view.read_with(cx, |view, _| {
        assert_eq!(view.selected_endpoint, 0);
        assert!(view.local_error.is_none());
    });
}

#[gpui_kit::test]
fn window_title_follows_the_focused_space_of_that_window(cx: &mut gpui_kit::TestAppContext) {
    let (view, cx) = crate::test_support::add_window_view(cx, fixture_window);
    cx.update(|window, cx| {
        view.update(cx, |view, _| {
            let mut snapshot = snapshot(4);
            snapshot.focused_workspace_id = Some("w0".into());
            view.live.snapshot = Some(Arc::new(snapshot));
        });
        view.update(cx, |view, _| view.sync_window_title(window));
    });
    assert_eq!(
        view.read_with(cx, |view, _| view.title.clone()),
        format!("{WINDOW_TITLE} \u{2014} herdr")
    );
    // An unknown focus falls back to the bare product name.
    cx.update(|window, cx| {
        view.update(cx, |view, _| {
            view.live.snapshot = None;
            view.sync_window_title(window);
        });
    });
    assert_eq!(
        view.read_with(cx, |view, _| view.title.clone()),
        WINDOW_TITLE
    );
}

#[gpui_kit::test]
fn the_sidebar_gap_narrows_the_terminal_only_while_the_sidebar_shows(
    cx: &mut gpui_kit::TestAppContext,
) {
    use gpui_kit::{px, size};

    let (view, cx) = crate::test_support::add_window_view(cx, fixture_window);
    cx.simulate_resize(size(px(900.), px(600.)));
    let draw = |cx: &mut gpui_kit::VisualTestContext| {
        cx.update(|window, cx| {
            window.refresh();
            window.draw(cx).clear(cx);
        });
    };
    let set_gap = |cx: &mut gpui_kit::VisualTestContext, gap: f32, visible: bool| {
        cx.update(|_, cx| {
            view.update(cx, |view, cx| {
                view.config.layout.sidebar_gap = gap;
                view.sidebar_visible = visible;
                cx.notify();
            });
        });
    };

    // The shipped default already separates the two panes.
    draw(cx);
    let sidebar = cx.debug_bounds("sidebar").unwrap();
    assert_eq!(
        view.read_with(cx, |view, _| view.bounds.origin.x),
        sidebar.right() + px(crate::config::Layout::default().sidebar_gap),
    );

    set_gap(cx, 0., true);
    draw(cx);
    let flush = view.read_with(cx, |view, _| view.bounds);
    assert_eq!(cx.debug_bounds("sidebar").unwrap(), sidebar);
    assert_eq!(flush.origin.x, sidebar.right());

    set_gap(cx, 16., true);
    draw(cx);
    let padded = view.read_with(cx, |view, _| view.bounds);
    // The same bounds feed painting, hit testing, and the resize the daemon
    // sees, so the gap must come out of the terminal's own width.
    assert_eq!(padded.origin.x, flush.origin.x + px(16.));
    assert_eq!(padded.size.width, flush.size.width - px(16.));
    assert_eq!(padded.size.height, flush.size.height);
    assert_eq!(cx.debug_bounds("sidebar").unwrap(), sidebar);

    // Hiding the sidebar leaves nothing to separate the terminal from.
    set_gap(cx, 16., false);
    draw(cx);
    let hidden = view.read_with(cx, |view, _| view.bounds);
    assert_eq!(hidden.origin.x, px(0.));
    assert_eq!(hidden.size.width, flush.size.width + sidebar.size.width);
}

#[gpui_kit::test]
fn a_held_key_repeats_in_the_terminal(cx: &mut gpui_kit::TestAppContext) {
    use crate::input::TerminalInputHandler;
    use gpui_kit::{Bounds, InputHandler};

    let (view, _) = crate::test_support::add_window_view(cx, fixture_window);
    // macOS sends a held key's repeats only when press-and-hold is off.
    let mut terminal = TerminalInputHandler::new(Bounds::default(), view);
    assert!(!terminal.apple_press_and_hold_enabled());
}
