use super::*;

/// Commands on the window itself reach the Settings window too, rather than
/// stopping at its root: Cmd-` from Settings moves on, and full screen works.
#[gpui::test]
fn settings_runs_window_commands(cx: &mut TestAppContext) {
    let source = cx.add_window(crate::sidebar::layout_tests::fixture_window);
    let weak = cx.update(|cx| source.update(cx, |_, _, cx| cx.weak_entity()).unwrap());
    let (_, cx) = cx.add_window_view(|window, cx| {
        let view = SettingsWindow::new(weak, cx);
        window.focus(&view.focus, cx);
        view
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.dispatch_action(crate::RunCommand {
        command: crate::controls::Command::ToggleFullScreen,
    });
    cx.update(|window, _| assert!(window.is_fullscreen()));
    cx.dispatch_action(crate::RunCommand {
        command: crate::controls::Command::CycleWindows,
    });
    cx.run_until_parked();
    cx.update(|_, cx| assert_eq!(cx.active_window(), Some(source.into())));
}

/// Close Window from Settings takes its own close path, so a layout chosen
/// there is saved rather than dropped with the window.
#[gpui::test]
fn close_window_saves_the_layout_before_closing(cx: &mut TestAppContext) {
    use crate::config::LayoutMode;
    let main = cx.add_window(crate::sidebar::layout_tests::fixture_window);
    let writes = Arc::new(Mutex::new(Vec::new()));
    let settings = cx.update(|cx| {
        let weak = main.update(cx, |_, _, cx| cx.weak_entity()).unwrap();
        open_fixture(weak, cx);
        let settings = cx.global::<SettingsWindowHandle>().window.unwrap();
        settings
            .update(cx, |view, window, cx| {
                view.layout_io = Some(recording_layouts(writes.clone(), false));
                view.accept_layout_choice(LayoutMode::Minimal, cx);
                assert!(writes.lock().unwrap().is_empty());
                window.dispatch_action(
                    Box::new(crate::RunCommand {
                        command: crate::controls::Command::CloseWindow,
                    }),
                    cx,
                );
            })
            .unwrap();
        settings
    });
    cx.run_until_parked();
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(10));
    cx.run_until_parked();
    assert_eq!(*writes.lock().unwrap(), [LayoutMode::Minimal]);
    assert!(cx.update(|cx| !cx.windows().contains(&settings.into())));
}
