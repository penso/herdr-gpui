use super::*;

/// A Settings window left open while Task Manager changes the entry reads
/// it again when it comes back to the front. Reads the real entry, never
/// writes it.
#[gpui::test]
fn coming_to_the_front_reads_the_startup_entry_again(cx: &mut TestAppContext) {
    let source = cx.add_window(crate::sidebar::layout_tests::fixture_window);
    let weak = cx.update(|cx| source.update(cx, |_, _, cx| cx.weak_entity()).unwrap());
    let (view, cx) = cx.add_window_view(|window, cx| {
        let mut view = SettingsWindow::new(weak, cx);
        view.watch_launch_at_login(window, cx);
        view
    });
    cx.run_until_parked();
    cx.deactivate_window();
    cx.run_until_parked();
    view.update(cx, |view, _| view.launch_at_login.state = None);
    // Inactive, nothing is read.
    cx.run_until_parked();
    assert_eq!(
        view.read_with(cx, |view, _| view.launch_at_login.state),
        None
    );

    cx.update(|window, _| window.activate_window());
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        assert!(view.launch_at_login.state.is_some());
        assert!(!view.launch_at_login.busy);
    });
}

#[gpui::test]
fn a_running_read_or_write_is_never_overlapped(cx: &mut TestAppContext) {
    let source = cx.add_window(crate::sidebar::layout_tests::fixture_window);
    let weak = cx.update(|cx| source.update(cx, |_, _, cx| cx.weak_entity()).unwrap());
    let (view, cx) = cx.add_window_view(|_, cx| SettingsWindow::new(weak, cx));
    view.update(cx, |view, cx| {
        view.launch_at_login.busy = true;
        // Skipped: a write here would race the running one.
        view.refresh_launch_at_login(Some(true), cx);
    });
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        assert!(view.launch_at_login.busy);
        assert_eq!(view.launch_at_login.state, None);
    });
}
