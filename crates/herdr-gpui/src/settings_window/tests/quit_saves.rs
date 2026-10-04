use super::*;

#[gpui::test]
fn close_and_quit_serialize_font_theme_and_final_layout(cx: &mut TestAppContext) {
    use crate::config::LayoutMode;
    for quit in [false, true] {
        let main = cx.add_window(crate::sidebar::layout_tests::fixture_window);
        let writes = Arc::new(Mutex::new(Vec::new()));
        let task = cx.update(|cx| {
            let weak = main.update(cx, |_, _, cx| cx.weak_entity()).unwrap();
            open_fixture(weak, cx);
            cx.global::<SettingsWindowHandle>()
                .window
                .unwrap()
                .update(cx, |view, window, cx| {
                    let sizes = writes.clone();
                    view.size_io = Some(SizeIo {
                        write: Arc::new(move |_| {
                            sizes.lock().unwrap().push("font");
                            Ok(())
                        }),
                        load: fixture_load,
                    });
                    let themes = writes.clone();
                    view.theme_io = Some(themes::ThemeIo {
                        write: Arc::new(move |name, _| {
                            assert_eq!(name, "Nord");
                            themes.lock().unwrap().push("theme");
                            Ok(())
                        }),
                        load: Arc::new(fixture_load),
                        resolve: None,
                    });
                    let layouts = writes.clone();
                    view.layout_io = Some(layouts::LayoutIo {
                        write: Arc::new(move |mode| {
                            assert_eq!(mode, LayoutMode::Minimal);
                            layouts.lock().unwrap().push("layout");
                            Ok(())
                        }),
                        load: Arc::new(fixture_load),
                    });
                    view.accept_control_size(FontFace::Sidebar, 30., cx);
                    view.accept_control_size(FontFace::Sidebar, 31., cx);
                    choose(view, "Nord", cx);
                    view.accept_layout_choice(LayoutMode::Orca, cx);
                    view.accept_layout_choice(LayoutMode::Minimal, cx);
                    assert!(writes.lock().unwrap().is_empty());
                    if quit {
                        let task = view.shutdown(cx);
                        window.remove_window();
                        Some(task)
                    } else {
                        view.close(window, cx);
                        None
                    }
                })
                .unwrap()
        });
        let (done, result) = std::sync::mpsc::sync_channel(1);
        if let Some(task) = task {
            cx.executor()
                .spawn(async move {
                    done.send(task.await).unwrap();
                })
                .detach();
        }
        cx.run_until_parked();
        cx.executor()
            .advance_clock(std::time::Duration::from_millis(10));
        cx.run_until_parked();
        if quit {
            result.try_recv().unwrap().unwrap();
        }
        assert_eq!(*writes.lock().unwrap(), ["font", "font", "theme", "layout"]);
        cx.update(|cx| {
            main.update(cx, |_, window, _| window.remove_window())
                .unwrap()
        });
    }
}

#[gpui::test]
fn quit_reports_layout_errors_including_an_inflight_close_save_without_duplicate_writes(
    cx: &mut TestAppContext,
) {
    use crate::config::LayoutMode;
    for inflight in [false, true] {
        let main = cx.add_window(crate::sidebar::layout_tests::fixture_window);
        let writes = Arc::new(Mutex::new(Vec::new()));
        let task = cx.update(|cx| {
            let weak = main.update(cx, |_, _, cx| cx.weak_entity()).unwrap();
            open_fixture(weak, cx);
            cx.global::<SettingsWindowHandle>()
                .window
                .unwrap()
                .update(cx, |view, window, cx| {
                    view.layout_io = Some(recording_layouts(writes.clone(), true));
                    view.accept_layout_choice(LayoutMode::Orca, cx);
                    if inflight {
                        view.close(window, cx);
                    }
                    let task = view.shutdown(cx);
                    view.accept_layout_choice(LayoutMode::Minimal, cx);
                    window.remove_window();
                    task
                })
                .unwrap()
        });
        let (done, result) = std::sync::mpsc::sync_channel(1);
        cx.executor()
            .spawn(async move {
                done.send(task.await).unwrap();
            })
            .detach();
        cx.run_until_parked();
        cx.executor()
            .advance_clock(std::time::Duration::from_millis(10));
        cx.run_until_parked();
        let error = result.try_recv().unwrap().unwrap_err();
        if inflight {
            assert!(
                matches!(error, crate::Error::SettingsSave(source) if matches!(*source, crate::Error::MissingHome))
            );
        } else {
            assert!(matches!(error, crate::Error::MissingHome));
        }
        assert_eq!(*writes.lock().unwrap(), [LayoutMode::Orca]);
        cx.update(|cx| {
            main.update(cx, |_, window, _| window.remove_window())
                .unwrap()
        });
    }
}

#[gpui::test]
fn quit_drains_latest_theme_behind_current_root_save(cx: &mut TestAppContext) {
    let source = cx.add_window(crate::sidebar::layout_tests::fixture_window);
    let writes = Arc::new(Mutex::new(Vec::new()));
    let quit = cx.update(|cx| {
        let weak = source.update(cx, |_, _, cx| cx.weak_entity()).unwrap();
        open_fixture(weak, cx);
        cx.global::<SettingsWindowHandle>()
            .window
            .unwrap()
            .update(cx, |view, window, cx| {
                view.theme_io = Some(recording_themes(writes.clone(), false));
                view.save_with(|| Ok(()), fixture_load, false, cx);
                choose(view, "Nord", cx);
                choose(view, "Dracula", cx);
                choose(view, "Default", cx);
                let quit = view.shutdown_with(|_| panic!("no font edits"), cx);
                window.remove_window();
                quit
            })
            .unwrap()
    });
    cx.run_until_parked();
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(10));
    cx.run_until_parked();
    assert_eq!(*writes.lock().unwrap(), ["Default"]);
    drop(quit);
}

#[gpui::test]
fn quit_waits_for_current_save_then_drains_latest_sizes_once(cx: &mut TestAppContext) {
    let source = cx.add_window(crate::sidebar::layout_tests::fixture_window);
    let writes = Arc::new(Mutex::new(Vec::new()));
    let (quit, weak) = cx.update(|cx| {
        let source = source.update(cx, |_, _, cx| cx.weak_entity()).unwrap();
        open_fixture(source, cx);
        let settings = cx.global::<SettingsWindowHandle>().window.unwrap();
        settings
            .update(cx, |view, window, cx| {
                view.size_io = Some(recording_sizes(writes.clone()));
                view.accept_control_size(FontFace::Terminal, 18., cx);
                assert!(view.saving);
                view.accept_control_size(FontFace::Terminal, 20., cx);
                view.accept_control_size(FontFace::Sidebar, 16., cx);
                view.accept_control_size(FontFace::Terminal, 24., cx);
                let quit = view.shutdown(cx);
                assert!(view.quitting);
                assert!(view.take_pending_control_sizes().is_empty());
                view.accept_control_size(FontFace::Terminal, 48., cx);
                assert!(view.take_pending_control_sizes().is_empty());
                window.remove_window();
                (quit, cx.weak_entity())
            })
            .unwrap()
    });
    cx.run_until_parked();
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(10));
    cx.run_until_parked();
    assert_eq!(
        *writes.lock().unwrap(),
        vec![
            vec![(FontFace::Terminal, 18.)],
            vec![(FontFace::Terminal, 24.), (FontFace::Sidebar, 16.)],
        ]
    );
    assert!(weak.upgrade().is_none());
    drop(quit);
}
