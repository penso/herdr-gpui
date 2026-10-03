use super::*;
use core::prelude::v1::test;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicUsize, Ordering},
};

fn fixture_load() -> crate::Result<Loaded> {
    Ok(fixture())
}

#[gpui::test]
fn footer_reload_stays_compact_and_ignores_busy_clicks(cx: &mut TestAppContext) {
    let main = cx.add_window(crate::sidebar::layout_tests::fixture_window);
    let weak = cx.update(|cx| main.update(cx, |_, _, cx| cx.weak_entity()).unwrap());
    let (view, cx) = cx.add_window_view(|_, cx| {
        let mut view = SettingsWindow::new(weak, cx);
        view.section = Section::General;
        view.error = Some(
            "A long settings error that must wrap without squeezing the Reload button. ".repeat(4),
        );
        view
    });
    let mut button_size = None;
    for width in [960., 680., 480.] {
        cx.simulate_resize(size(px(width), px(560.)));
        cx.update(|window, cx| crate::sidebar::layout_tests::full_draw(window, cx).clear(cx));
        let button = cx.debug_bounds("settings-footer-reload").unwrap();
        let status = cx.debug_bounds("settings-footer-status").unwrap();
        assert!(button.size.width >= px(50.));
        assert!(button.size.height >= px(24.) && button.size.height <= px(28.));
        assert!(button.right() <= px(width - 16.));
        assert!(button.bottom() <= px(560. - 8.));
        assert!(status.right() + px(12.) <= button.left());
        assert!(status.size.width > px(0.));
        assert_eq!(*button_size.get_or_insert(button.size), button.size);
    }
    for busy in 0..3 {
        view.update(cx, |view, cx| {
            view.loading = busy == 0;
            view.saving = busy == 1;
            view.quitting = busy == 2;
            cx.notify();
        });
        cx.update(|window, cx| crate::sidebar::layout_tests::full_draw(window, cx).clear(cx));
        let button = cx.debug_bounds("settings-footer-reload").unwrap();
        cx.simulate_click(button.center(), Default::default());
        cx.run_until_parked();
        view.read_with(cx, |view, _| {
            assert_eq!(view.loading, busy == 0);
            assert_eq!(view.saving, busy == 1);
            assert_eq!(view.quitting, busy == 2);
            assert!(view.error.is_some(), "busy reload must not clear the error");
        });
    }
}

type SizeWrites = Arc<Mutex<Vec<Vec<(FontFace, f32)>>>>;

fn recording_sizes(writes: SizeWrites) -> SizeIo {
    SizeIo {
        write: Arc::new(move |sizes| {
            writes.lock().unwrap().push(sizes);
            Ok(())
        }),
        load: fixture_load,
    }
}

fn recording_themes(writes: Arc<Mutex<Vec<String>>>, fail: bool) -> themes::ThemeIo {
    let disk = Arc::new(Mutex::new("Default".to_owned()));
    let saved = disk.clone();
    themes::ThemeIo {
        resolve: None,
        write: Arc::new(move |name, shared| {
            assert!(shared.is_none());
            writes.lock().unwrap().push(name.clone());
            if fail {
                return Err(crate::Error::MissingHome);
            }
            *saved.lock().unwrap() = name;
            Ok(())
        }),
        load: Arc::new(move || {
            let mut loaded = fixture();
            loaded.config.theme = disk.lock().unwrap().clone();
            loaded.theme = loaded.config.theme()?;
            Ok(loaded)
        }),
    }
}

fn recording_layouts(
    writes: Arc<Mutex<Vec<crate::config::LayoutMode>>>,
    fail: bool,
) -> layouts::LayoutIo {
    let disk = Arc::new(Mutex::new(Config::default().layout.mode));
    let saved = disk.clone();
    layouts::LayoutIo {
        write: Arc::new(move |mode| {
            writes.lock().unwrap().push(mode);
            if fail {
                return Err(crate::Error::MissingHome);
            }
            *saved.lock().unwrap() = mode;
            Ok(())
        }),
        load: Arc::new(move || {
            let mut loaded = fixture();
            loaded.config.layout.mode = *disk.lock().unwrap();
            Ok(loaded)
        }),
    }
}

#[gpui::test]
fn layout_clicks_apply_live_without_writing_and_close_commits_only_final(cx: &mut TestAppContext) {
    use crate::config::LayoutMode;
    let first = cx.add_window(crate::sidebar::layout_tests::fixture_window);
    let second = cx.add_window(crate::sidebar::layout_tests::fixture_window);
    let writes = Arc::new(Mutex::new(Vec::new()));
    let weak = cx.update(|cx| first.update(cx, |_, _, cx| cx.weak_entity()).unwrap());
    let (view, cx) = cx.add_window_view(|window, cx| {
        cx.bind_keys(key_bindings());
        let mut view = SettingsWindow::new(weak, cx);
        view.layout_io = Some(recording_layouts(writes.clone(), false));
        window.focus(&view.focus, cx);
        view
    });
    cx.simulate_resize(size(px(960.), px(2200.)));
    let revision = cx.update(|_, cx| layout_load_revision(cx));
    for (mode, selector) in [
        (LayoutMode::Orca, "settings-layout-orca"),
        (LayoutMode::Minimal, "settings-layout-minimal"),
        (LayoutMode::Superset, "settings-layout-superset"),
    ] {
        cx.update(|window, cx| crate::sidebar::layout_tests::full_draw(window, cx).clear(cx));
        let bounds = cx.debug_bounds(selector).unwrap();
        cx.simulate_click(bounds.center(), Default::default());
        cx.run_until_parked();
        assert!(writes.lock().unwrap().is_empty());
        view.read_with(cx, |view, cx| {
            assert_eq!(view.layout_intent, Some(mode));
            assert_eq!(view.config.layout.mode, mode);
            assert!(!view.busy());
            assert!(view.save_completion.is_none());
            for main in [first, second] {
                assert_eq!(main.read(cx).unwrap().config.layout.mode, mode);
            }
            assert_eq!(
                cx.global::<crate::app::InitialAppearance>()
                    .config
                    .layout
                    .mode,
                mode
            );
        });
    }
    cx.simulate_keystrokes("cmd-w");
    cx.run_until_parked();
    assert_eq!(*writes.lock().unwrap(), [LayoutMode::Superset]);
    assert_eq!(cx.windows().len(), 2);
    view.read_with(cx, |_, cx| {
        let mut stale = Config::default();
        apply_loaded_layout(&mut stale, revision, cx);
        assert_eq!(stale.layout.mode, LayoutMode::Superset);
        let mut fresh = Config::default();
        apply_loaded_layout(&mut fresh, layout_load_revision(cx), cx);
        assert_eq!(fresh.layout.mode, Config::default().layout.mode);
    });
}

#[gpui::test]
fn layout_draft_survives_settings_and_main_reloads_and_other_saves(cx: &mut TestAppContext) {
    use crate::config::LayoutMode;
    let main = cx.add_window(crate::sidebar::layout_tests::fixture_window);
    let writes = Arc::new(Mutex::new(Vec::new()));
    let settings = cx.update(|cx| {
        let weak = main
            .update(cx, |view, _, cx| {
                view.load_gui_config_with(|| Ok((Config::default(), Theme::default())), cx);
                cx.weak_entity()
            })
            .unwrap();
        open_fixture(weak, cx);
        let settings = cx.global::<SettingsWindowHandle>().window.unwrap();
        settings
            .update(cx, |view, _, cx| {
                view.layout_io = Some(recording_layouts(writes.clone(), false));
                view.save_with(
                    || Ok(()),
                    || {
                        let mut loaded = fixture();
                        loaded.config.ui.size = 23.;
                        Ok(loaded)
                    },
                    false,
                    cx,
                );
                view.accept_layout_choice(LayoutMode::Orca, cx);
            })
            .unwrap();
        settings
    });
    cx.run_until_parked();
    cx.update(|cx| {
        let view = settings.read(cx).unwrap();
        assert_eq!(view.config.ui.size, 23.);
        assert_eq!(view.config.layout.mode, LayoutMode::Orca);
        assert_eq!(main.read(cx).unwrap().config.layout.mode, LayoutMode::Orca);
        settings
            .update(cx, |view, _, cx| view.reload_with(fixture_load, cx))
            .unwrap();
    });
    cx.run_until_parked();
    cx.update(|cx| {
        let view = settings.read(cx).unwrap();
        assert_eq!(view.config.layout.mode, LayoutMode::Orca);
        assert_eq!(view.layout_intent, Some(LayoutMode::Orca));
        assert_eq!(
            cx.global::<crate::app::InitialAppearance>()
                .config
                .layout
                .mode,
            LayoutMode::Orca
        );
    });
    assert!(writes.lock().unwrap().is_empty());
}

#[gpui::test]
fn layout_close_failure_retains_live_draft_until_explicit_retry(cx: &mut TestAppContext) {
    use crate::config::LayoutMode;
    let main = cx.add_window(crate::sidebar::layout_tests::fixture_window);
    let writes = Arc::new(Mutex::new(Vec::new()));
    let settings = cx.update(|cx| {
        let weak = main.update(cx, |_, _, cx| cx.weak_entity()).unwrap();
        open_fixture(weak, cx);
        let settings = cx.global::<SettingsWindowHandle>().window.unwrap();
        settings
            .update(cx, |view, window, cx| {
                view.layout_io = Some(recording_layouts(writes.clone(), true));
                view.accept_layout_choice(LayoutMode::Orca, cx);
                assert!(!view.should_close(window, cx));
                assert!(!view.should_close(window, cx));
                view.accept_layout_choice(LayoutMode::Minimal, cx);
            })
            .unwrap();
        settings
    });
    cx.run_until_parked();
    assert_eq!(*writes.lock().unwrap(), [LayoutMode::Orca]);
    cx.update(|cx| {
        settings
            .update(cx, |view, _, cx| {
                assert!(view.closing.is_none());
                assert!(!view.busy());
                assert!(view.error.as_ref().unwrap().contains("Save settings"));
                assert_eq!(view.layout_intent, Some(LayoutMode::Orca));
                assert_eq!(view.config.layout.mode, LayoutMode::Orca);
                assert_eq!(main.read(cx).unwrap().config.layout.mode, LayoutMode::Orca);
                view.reload_with(fixture_load, cx);
            })
            .unwrap();
    });
    cx.run_until_parked();
    assert_eq!(*writes.lock().unwrap(), [LayoutMode::Orca]);
    cx.update(|cx| {
        settings
            .update(cx, |view, window, cx| {
                view.layout_io = Some(recording_layouts(writes.clone(), false));
                view.close(window, cx);
            })
            .unwrap()
    });
    cx.run_until_parked();
    cx.update(|cx| assert!(settings.read(cx).is_err()));
    assert_eq!(
        *writes.lock().unwrap(),
        [LayoutMode::Orca, LayoutMode::Orca]
    );
}

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

fn choose(view: &mut SettingsWindow, name: &str, cx: &mut Context<SettingsWindow>) {
    view.accept_theme_choice(
        themes::Choice {
            scope: themes::Scope::App,
            name: name.into(),
        },
        cx,
    );
}

#[gpui::test]
fn keyboard_close_writes_final_draft_once_and_browsing_never_reloads_config(
    cx: &mut TestAppContext,
) {
    let source = cx.add_window(crate::sidebar::layout_tests::fixture_window);
    let weak = cx.update(|cx| source.update(cx, |_, _, cx| cx.weak_entity()).unwrap());
    let writes = Arc::new(Mutex::new(Vec::new()));
    let reloads = Arc::new(AtomicUsize::new(0));
    let (view, cx) = cx.add_window_view(|window, cx| {
        cx.bind_keys(key_bindings());
        let mut view = SettingsWindow::new(weak, cx);
        let mut io = recording_themes(writes.clone(), false);
        let load = io.load.clone();
        let reloads = reloads.clone();
        io.load = Arc::new(move || {
            reloads.fetch_add(1, Ordering::SeqCst);
            load()
        });
        view.theme_io = Some(io);
        window.focus(&view.focus, cx);
        view
    });
    for name in ["Default", "Dracula", "Nord"] {
        view.update(cx, |view, cx| choose(view, name, cx));
        cx.run_until_parked();
        assert!(writes.lock().unwrap().is_empty());
        assert_eq!(reloads.load(Ordering::SeqCst), 0);
        view.read_with(cx, |view, _| assert!(view.theme_dirty()));
    }
    cx.simulate_keystrokes("cmd-w");
    cx.run_until_parked();
    assert_eq!(*writes.lock().unwrap(), ["Nord"]);
    assert_eq!(reloads.load(Ordering::SeqCst), 1);
    assert_eq!(cx.windows(), [source.into()]);
}

#[gpui::test]
fn non_theme_save_preserves_draft_and_reapplies_contrast_once(cx: &mut TestAppContext) {
    let source = cx.add_window(crate::sidebar::layout_tests::fixture_window);
    let writes = Arc::new(Mutex::new(Vec::new()));
    let settings = cx.update(|cx| {
        let weak = source.update(cx, |_, _, cx| cx.weak_entity()).unwrap();
        open_fixture(weak, cx);
        let settings = cx.global::<SettingsWindowHandle>().window.unwrap();
        settings
            .update(cx, |view, _, cx| {
                view.theme_io = Some(recording_themes(writes.clone(), false));
                choose(view, "Nord", cx);
                view.save_with(
                    || Ok(()),
                    || {
                        let mut loaded = fixture();
                        loaded.config.ui.size = 22.;
                        loaded.config.contrast = crate::contrast::Contrast::High;
                        Ok(loaded)
                    },
                    false,
                    cx,
                );
            })
            .unwrap();
        settings
    });
    cx.run_until_parked();
    assert!(writes.lock().unwrap().is_empty());
    cx.update(|cx| {
        let view = settings.read(cx).unwrap();
        assert_eq!(view.config.ui.size, 22.);
        assert_eq!(view.config.theme, "Nord");
        let expected = Theme::builtin("Nord")
            .unwrap()
            .with_contrast(crate::contrast::Contrast::High);
        assert_eq!(view.theme, expected);
        assert_eq!(source.read(cx).unwrap().theme, expected);
        assert!(view.theme_dirty());
        assert!(!view.saving);
    });
}

#[gpui::test]
fn close_while_latest_external_choice_loads_saves_only_that_choice(cx: &mut TestAppContext) {
    let source = cx.add_window(crate::sidebar::layout_tests::fixture_window);
    let writes = Arc::new(Mutex::new(Vec::new()));
    let loads = Arc::new(Mutex::new(Vec::new()));
    let settings = cx.update(|cx| {
        let weak = source.update(cx, |_, _, cx| cx.weak_entity()).unwrap();
        open_fixture(weak, cx);
        let settings = cx.global::<SettingsWindowHandle>().window.unwrap();
        settings
            .update(cx, |view, window, cx| {
                let writes = writes.clone();
                let loads = loads.clone();
                view.theme_io = Some(themes::ThemeIo {
                    write: Arc::new(move |name, _| {
                        writes.lock().unwrap().push(name);
                        Ok(())
                    }),
                    resolve: Some(Arc::new(move |name| {
                        loads.lock().unwrap().push(name.to_owned());
                        Ok(Theme::builtin("Nord").unwrap())
                    })),
                    load: Arc::new(|| {
                        let mut loaded = fixture();
                        loaded.config.theme = "external-c".into();
                        loaded.theme = Theme::builtin("Nord").unwrap();
                        Ok(loaded)
                    }),
                });
                for name in ["external-a", "external-b", "external-c"] {
                    choose(view, name, cx);
                }
                assert!(!view.should_close(window, cx));
                assert!(!view.saving);
                assert!(view.theme_loading);
            })
            .unwrap();
        settings
    });
    cx.run_until_parked();
    assert_eq!(*loads.lock().unwrap(), ["external-a", "external-c"]);
    assert_eq!(*writes.lock().unwrap(), ["external-c"]);
    cx.update(|cx| {
        assert!(settings.read(cx).is_err());
        assert_eq!(
            source.read(cx).unwrap().theme,
            Theme::builtin("Nord").unwrap()
        );
        assert!(!theme_pending(cx));
    });
}

#[gpui::test]
fn theme_draft_paints_all_windows_and_allows_non_theme_reload(cx: &mut TestAppContext) {
    let first = cx.add_window(crate::sidebar::layout_tests::fixture_window);
    let second = cx.add_window(crate::sidebar::layout_tests::fixture_window);
    let writes = Arc::new(Mutex::new(Vec::new()));
    cx.update(|cx| {
        cx.set_global(crate::app::InitialAppearance::default());
        let source = first
            .update(cx, |view, _, cx| {
                view.config.terminal.size = 29.;
                view.load_gui_config_with(
                    || {
                        let mut config = Config::default();
                        config.ui.size = 21.;
                        Ok((config, Theme::default()))
                    },
                    cx,
                );
                cx.weak_entity()
            })
            .unwrap();
        second
            .update(cx, |view, _, _| view.config.ui.size = 23.)
            .unwrap();
        open_fixture(source, cx);
        cx.global::<SettingsWindowHandle>()
            .window
            .unwrap()
            .update(cx, |view, _, cx| {
                view.theme_io = Some(recording_themes(writes.clone(), false));
                choose(view, "Nord", cx);
                assert!(!view.saving);
                assert!(view.theme_dirty());
                assert_eq!(view.theme, Theme::builtin("Nord").unwrap());
                assert!(writes.lock().unwrap().is_empty());
            })
            .unwrap();
        for main in [first, second] {
            assert_eq!(
                main.read(cx).unwrap().theme,
                Theme::builtin("Nord").unwrap()
            );
            assert_eq!(main.read(cx).unwrap().config.theme, "Nord");
        }
        assert_eq!(first.read(cx).unwrap().config.terminal.size, 29.);
        assert_eq!(second.read(cx).unwrap().config.ui.size, 23.);
        assert_eq!(
            cx.global::<crate::app::InitialAppearance>().theme,
            Theme::builtin("Nord").unwrap()
        );
    });
    cx.run_until_parked();
    assert!(writes.lock().unwrap().is_empty());
    cx.update(|cx| {
        assert!(theme_pending(cx));
        assert_eq!(first.read(cx).unwrap().config.ui.size, 21.);
        assert_eq!(second.read(cx).unwrap().config.ui.size, 23.);
        assert_eq!(
            first.read(cx).unwrap().theme,
            Theme::builtin("Nord").unwrap()
        );
    });
}

#[gpui::test]
fn native_close_saves_only_final_theme_after_current_root_save(cx: &mut TestAppContext) {
    let source = cx.add_window(crate::sidebar::layout_tests::fixture_window);
    let writes = Arc::new(Mutex::new(Vec::new()));
    cx.update(|cx| {
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
                assert_eq!(view.theme, Theme::default());
                assert!(writes.lock().unwrap().is_empty());
                assert!(!view.should_close(window, cx));
                assert!(!view.should_close(window, cx));
            })
            .unwrap();
    });
    cx.run_until_parked();
    assert_eq!(*writes.lock().unwrap(), ["Default"]);
    cx.update(|cx| {
        assert!(!theme_pending(cx));
        assert_eq!(source.read(cx).unwrap().theme, Theme::default());
        assert!(
            cx.global::<SettingsWindowHandle>()
                .model
                .as_ref()
                .unwrap()
                .upgrade()
                .is_none()
        );
    });
}

#[gpui::test]
fn theme_close_failure_keeps_window_and_draft_without_retrying(cx: &mut TestAppContext) {
    let source = cx.add_window(crate::sidebar::layout_tests::fixture_window);
    let writes = Arc::new(Mutex::new(Vec::new()));
    let settings = cx.update(|cx| {
        let weak = source.update(cx, |_, _, cx| cx.weak_entity()).unwrap();
        open_fixture(weak, cx);
        let settings = cx.global::<SettingsWindowHandle>().window.unwrap();
        settings
            .update(cx, |view, window, cx| {
                view.theme_io = Some(recording_themes(writes.clone(), true));
                choose(view, "Nord", cx);
                assert_eq!(view.theme, Theme::builtin("Nord").unwrap());
                view.close(window, cx);
            })
            .unwrap();
        settings
    });
    cx.run_until_parked();
    assert_eq!(*writes.lock().unwrap(), ["Nord"]);
    cx.update(|cx| {
        let view = settings.read(cx).unwrap();
        assert_eq!(view.theme, Theme::builtin("Nord").unwrap());
        assert!(view.error.as_ref().unwrap().contains("Save settings"));
        assert!(!view.saving);
        assert!(theme_pending(cx));
        assert!(view.theme_dirty());
        assert!(view.closing.is_none());
        assert_eq!(
            source.read(cx).unwrap().theme,
            Theme::builtin("Nord").unwrap()
        );
        assert_eq!(
            cx.global::<crate::app::InitialAppearance>().theme,
            Theme::builtin("Nord").unwrap()
        );
    });
    cx.run_until_parked();
    assert_eq!(*writes.lock().unwrap(), ["Nord"]);
    cx.update(|cx| {
        settings
            .update(cx, |view, window, cx| {
                view.theme_io = Some(recording_themes(writes.clone(), false));
                view.close(window, cx);
            })
            .unwrap()
    });
    cx.run_until_parked();
    cx.update(|cx| assert!(settings.read(cx).is_err()));
    assert_eq!(*writes.lock().unwrap(), ["Nord", "Nord"]);
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

#[cfg(unix)]
#[gpui::test]
fn quit_shared_theme_uses_only_successfully_reconciled_preceding_snapshot(cx: &mut TestAppContext) {
    // Failed writes/loads must retain the original conflict boundary. A genuine
    // conflict after a successful handoff must surface, never reload and retry.
    for (saved_ok, reload_ok, shared_available, external_conflict) in [
        (true, true, true, false),
        (false, true, true, false),
        (true, false, true, false),
        (true, true, false, false),
        (true, true, true, true),
    ] {
        let source = cx.add_window(crate::sidebar::layout_tests::fixture_window);
        let operations = Arc::new(Mutex::new(Vec::new()));
        let refreshed = saved_ok && reload_ok && shared_available;
        let quit = cx.update(|cx| {
            let weak = source.update(cx, |_, _, cx| cx.weak_entity()).unwrap();
            open_fixture(weak, cx);
            cx.global::<SettingsWindowHandle>()
                .window
                .unwrap()
                .update(cx, |view, window, cx| {
                    let original = herdr_settings::Settings::parse_text(
                        "[ui.sound]\nenabled=true\n[theme.custom]\naccent='#123456'\n",
                    )
                    .unwrap();
                    let updated = herdr_settings::Settings::parse_text(
                        "[ui.sound]\nenabled=false\n[theme.custom]\naccent='#abcdef'\n",
                    )
                    .unwrap();
                    let expected = if refreshed {
                        updated.clone()
                    } else {
                        original.clone()
                    };
                    view.shared = Some(original);
                    let written = operations.clone();
                    view.theme_io = Some(themes::ThemeIo {
                        write: Arc::new(move |name, shared| {
                            let shared = shared.unwrap();
                            assert_eq!(name, "nord");
                            assert_eq!(shared.sound_enabled, expected.sound_enabled);
                            assert_eq!(
                                shared.theme(false).unwrap(),
                                expected.theme(false).unwrap()
                            );
                            let mut operations = written.lock().unwrap();
                            assert_eq!(*operations, ["preceding write"]);
                            operations.push("theme write");
                            if external_conflict {
                                Err(herdr_settings::Error::Conflict.into())
                            } else {
                                Ok(())
                            }
                        }),
                        load: Arc::new(|| {
                            panic!("shutdown must not reload/retry the theme writer")
                        }),
                        resolve: None,
                    });
                    let preceding = operations.clone();
                    view.save_with(
                        move || {
                            preceding.lock().unwrap().push("preceding write");
                            if saved_ok {
                                Ok(())
                            } else {
                                Err(crate::Error::MissingHome)
                            }
                        },
                        move || {
                            if !reload_ok {
                                return Err(crate::Error::MissingHome);
                            }
                            let mut loaded = fixture();
                            loaded.shared = shared_available.then_some(updated);
                            Ok(loaded)
                        },
                        true,
                        cx,
                    );
                    view.accept_theme_choice(
                        themes::Choice {
                            scope: themes::Scope::Herdr,
                            name: "nord".into(),
                        },
                        cx,
                    );
                    let quit = view.shutdown_with(|_| panic!("no pending font sizes"), cx);
                    window.remove_window();
                    quit
                })
                .unwrap()
        });
        let (done, result) = std::sync::mpsc::sync_channel(1);
        cx.executor()
            .spawn(async move {
                done.send(quit.await).unwrap();
            })
            .detach();
        cx.run_until_parked();
        cx.executor()
            .advance_clock(std::time::Duration::from_millis(10));
        cx.run_until_parked();
        let result = result.try_recv().unwrap();
        if external_conflict {
            let crate::Error::ConfigFile { source, .. } = result.unwrap_err() else {
                panic!("expected typed shared conflict")
            };
            assert!(matches!(
                source.downcast_ref::<herdr_settings::Error>(),
                Some(herdr_settings::Error::Conflict)
            ));
        } else if saved_ok {
            result.unwrap();
        } else {
            assert!(matches!(result, Err(crate::Error::SettingsSave(source))
                if matches!(*source, crate::Error::MissingHome)));
        }
        assert_eq!(
            *operations.lock().unwrap(),
            ["preceding write", "theme write"]
        );
        cx.update(|cx| {
            source
                .update(cx, |_, window, _| window.remove_window())
                .unwrap();
            themes::clear_theme_draft(cx);
        });
    }
}

#[gpui::test]
fn shared_theme_edit_support_does_not_restrict_native_or_follow_choices(cx: &mut TestAppContext) {
    let source = cx.add_window(crate::sidebar::layout_tests::fixture_window);
    cx.update(|cx| {
        let weak = source.update(cx, |_, _, cx| cx.weak_entity()).unwrap();
        open_fixture(weak, cx);
        cx.global::<SettingsWindowHandle>()
            .window
            .unwrap()
            .update(cx, |view, _, cx| {
                view.shared =
                    Some(herdr_settings::Settings::parse_text("[theme]\nname='nord'\n").unwrap());
                view.config.theme = "Follow Herdr".into();
                view.theme = view.shared.as_ref().unwrap().theme(false).unwrap();
                let original = view.theme.clone();
                let main_theme = source.read(cx).unwrap().theme.clone();
                view.accept_theme_choice(
                    themes::Choice {
                        scope: themes::Scope::Herdr,
                        name: "dracula".into(),
                    },
                    cx,
                );
                assert_eq!(view.theme_dirty(), cfg!(unix));
                assert_eq!(theme_pending(cx), cfg!(unix));
                if cfg!(unix) {
                    assert_ne!(view.theme, original);
                } else {
                    assert_eq!(view.theme, original);
                    assert_eq!(source.read(cx).unwrap().theme, main_theme);
                    assert!(view.status.as_ref().unwrap().contains("read-only"));
                    assert!(!view.theme_loading);
                    assert!(!view.saving);
                }
                choose(view, "Nord", cx);
                assert!(view.theme_dirty());
                assert_eq!(view.config.theme, "Nord");
                choose(view, "Follow Herdr", cx);
                assert!(view.theme_dirty());
                assert_eq!(view.config.theme, "Follow Herdr");
            })
            .unwrap();
    });
}

#[gpui::test]
fn external_theme_loads_coalesce_and_only_latest_validated_result_paints(cx: &mut TestAppContext) {
    let source = cx.add_window(crate::sidebar::layout_tests::fixture_window);
    let loads = Arc::new(Mutex::new(Vec::new()));
    let writes = Arc::new(Mutex::new(Vec::new()));
    let settings = cx.update(|cx| {
        let weak = source.update(cx, |_, _, cx| cx.weak_entity()).unwrap();
        open_fixture(weak, cx);
        let settings = cx.global::<SettingsWindowHandle>().window.unwrap();
        settings
            .update(cx, |view, _, cx| {
                let loads = loads.clone();
                let writes = writes.clone();
                view.theme_io = Some(themes::ThemeIo {
                    resolve: Some(Arc::new(move |name| {
                        loads.lock().unwrap().push(name.to_owned());
                        Ok(Theme::builtin(if name == "external-c" {
                            "Dracula"
                        } else {
                            "Nord"
                        })
                        .unwrap())
                    })),
                    write: Arc::new(move |name, _| {
                        writes.lock().unwrap().push(name);
                        Ok(())
                    }),
                    load: Arc::new(|| {
                        let mut loaded = fixture();
                        loaded.config.theme = "external-c".into();
                        loaded.theme = Theme::builtin("Dracula").unwrap();
                        Ok(loaded)
                    }),
                });
                for name in ["external-a", "external-b", "external-c"] {
                    choose(view, name, cx);
                }
                assert_eq!(view.theme, Theme::default());
            })
            .unwrap();
        settings
    });
    cx.run_until_parked();
    assert_eq!(*loads.lock().unwrap(), ["external-a", "external-c"]);
    assert!(writes.lock().unwrap().is_empty());
    cx.update(|cx| {
        settings
            .update(cx, |view, _, cx| {
                choose(view, "Nord", cx);
                choose(view, "external-c", cx);
                assert!(
                    !view.theme_loading,
                    "returning to a validated definition uses the cache"
                );
            })
            .unwrap();
        assert_eq!(
            settings.read(cx).unwrap().theme,
            Theme::builtin("Dracula").unwrap()
        );
        assert_eq!(
            source.read(cx).unwrap().theme,
            Theme::builtin("Dracula").unwrap()
        );
        settings
            .update(cx, |view, window, cx| view.close(window, cx))
            .unwrap();
    });
    cx.run_until_parked();
    assert_eq!(*writes.lock().unwrap(), ["external-c"]);
    assert_eq!(*loads.lock().unwrap(), ["external-a", "external-c"]);
}

#[gpui::test]
fn failed_definition_keeps_close_pending_draft_for_explicit_retry(cx: &mut TestAppContext) {
    let source = cx.add_window(crate::sidebar::layout_tests::fixture_window);
    let loads = Arc::new(AtomicUsize::new(0));
    let settings = cx.update(|cx| {
        let weak = source.update(cx, |_, _, cx| cx.weak_entity()).unwrap();
        open_fixture(weak, cx);
        let settings = cx.global::<SettingsWindowHandle>().window.unwrap();
        settings
            .update(cx, |view, window, cx| {
                let loads = loads.clone();
                view.theme_io = Some(themes::ThemeIo {
                    write: Arc::new(|_, _| panic!("invalid definitions must not be persisted")),
                    load: Arc::new(|| panic!("validation must not reload config")),
                    resolve: Some(Arc::new(move |_| {
                        loads.fetch_add(1, Ordering::SeqCst);
                        Err(crate::Error::MissingHome)
                    })),
                });
                choose(view, "missing-definition", cx);
                assert!(!view.should_close(window, cx));
            })
            .unwrap();
        settings
    });
    cx.run_until_parked();
    cx.run_until_parked();
    assert_eq!(loads.load(Ordering::SeqCst), 1);
    cx.update(|cx| {
        let view = settings.read(cx).unwrap();
        assert!(view.theme_dirty());
        assert!(view.closing.is_none());
        assert!(view.error.as_ref().unwrap().contains("Load theme"));
        source
            .update(cx, |view, _, cx| {
                view.load_gui_config_with(
                    || {
                        let mut config = Config::default();
                        config.ui.size = 23.;
                        Ok((config, Theme::default()))
                    },
                    cx,
                );
            })
            .unwrap();
    });
    cx.run_until_parked();
    assert_eq!(loads.load(Ordering::SeqCst), 1);
    cx.update(|cx| assert_eq!(source.read(cx).unwrap().config.ui.size, 23.));
}

#[gpui::test]
fn accepted_close_survives_source_close_and_reactivation_and_fences_late_theme_reads(
    cx: &mut TestAppContext,
) {
    let source = cx.add_window(crate::sidebar::layout_tests::fixture_window);
    let writes = Arc::new(Mutex::new(Vec::new()));
    let (settings, revision) = cx.update(|cx| {
        let weak = source.update(cx, |_, _, cx| cx.weak_entity()).unwrap();
        open_fixture(weak.clone(), cx);
        let settings = cx.global::<SettingsWindowHandle>().window.unwrap();
        source
            .update(cx, |_, window, _| window.remove_window())
            .unwrap();
        let revision = theme_load_revision(cx);
        settings
            .update(cx, |view, window, cx| {
                view.theme_io = Some(recording_themes(writes.clone(), false));
                choose(view, "Nord", cx);
                view.close(window, cx);
                assert!(view.saving);
            })
            .unwrap();
        open_fixture(weak, cx);
        assert_eq!(cx.global::<SettingsWindowHandle>().window, Some(settings));
        (settings, revision)
    });
    cx.run_until_parked();
    assert_eq!(*writes.lock().unwrap(), ["Nord"]);
    cx.update(|cx| {
        assert!(settings.read(cx).is_err());
        assert!(!theme_pending(cx));
        let mut config = Config::default();
        config.ui.size = 25.;
        let mut theme = Theme::default();
        apply_loaded_theme(&mut config, &mut theme, revision, cx);
        assert_eq!(config.ui.size, 25.);
        assert_eq!(config.theme, "Nord");
        assert_eq!(theme, Theme::builtin("Nord").unwrap());
    });
}

#[gpui::test]
fn theme_selection_cancels_main_picker_and_preserves_focus(cx: &mut TestAppContext) {
    let source = cx.add_window(crate::sidebar::layout_tests::fixture_window);
    let writes = Arc::new(Mutex::new(Vec::new()));
    cx.update(|cx| {
        let weak = source
            .update(cx, |view, window, cx| {
                view.open_theme_picker(window, cx);
                cx.weak_entity()
            })
            .unwrap();
        open_fixture(weak, cx);
        cx.global::<SettingsWindowHandle>()
            .window
            .unwrap()
            .update(cx, |view, _, cx| {
                view.theme_io = Some(recording_themes(writes, false));
                choose(view, "Nord", cx);
            })
            .unwrap();
        source
            .update(cx, |view, window, cx| {
                assert!(view.menu.page.is_none());
                assert_eq!(view.theme, Theme::builtin("Nord").unwrap());
                view.dismiss_menu(window, cx);
                assert_eq!(view.theme, Theme::builtin("Nord").unwrap());
            })
            .unwrap();
    });
    cx.run_until_parked();
}

#[gpui::test]
fn shared_selection_preserves_native_override_and_follow_uses_prepared_colors(
    cx: &mut TestAppContext,
) {
    let source = cx.add_window(crate::sidebar::layout_tests::fixture_window);
    cx.update(|cx| {
        let weak = source.update(cx, |_, _, cx| cx.weak_entity()).unwrap();
        open_fixture(weak, cx);
        cx.global::<SettingsWindowHandle>()
            .window
            .unwrap()
            .update(cx, |view, _, cx| {
                view.shared = Some(
                    herdr_settings::Settings::parse_text(
                        "[theme]\nname='nord'\n[theme.custom]\naccent='#123456'\n",
                    )
                    .unwrap(),
                );
                view.saving = true; // Hold the root slot, without scheduling any real I/O.
                view.accept_theme_choice(
                    themes::Choice {
                        scope: themes::Scope::Herdr,
                        name: "dracula".into(),
                    },
                    cx,
                );
                assert_eq!(view.config.theme, "Default");
                assert_eq!(view.theme, Theme::default());
                assert_eq!(source.read(cx).unwrap().theme, Theme::default());
                choose(view, "Follow Herdr", cx);
                let expected = view
                    .shared
                    .as_ref()
                    .unwrap()
                    .theme(view.theme_light)
                    .unwrap();
                assert_eq!(view.theme, expected);
                assert_eq!(view.theme.primary(), 0x123456);
                assert_eq!(source.read(cx).unwrap().theme, expected);
                assert_eq!(source.read(cx).unwrap().config.theme, "Follow Herdr");
            })
            .unwrap();
    });
}

#[gpui::test]
fn explicit_theme_applies_contrast_once_and_shared_callbacks_cannot_restore_old_palette(
    cx: &mut TestAppContext,
) {
    let source = cx.add_window(crate::sidebar::layout_tests::fixture_window);
    let expected = Theme::builtin("Nord")
        .unwrap()
        .with_contrast(crate::contrast::Contrast::High);
    let settings = cx.update(|cx| {
        let weak = source.update(cx, |_, _, cx| cx.weak_entity()).unwrap();
        open_fixture(weak, cx);
        let settings = cx.global::<SettingsWindowHandle>().window.unwrap();
        settings
            .update(cx, |view, _, cx| {
                view.config.contrast = crate::contrast::Contrast::High;
                view.saving = true;
                choose(view, "Nord", cx);
                assert_eq!(view.theme, expected);
                assert_eq!(source.read(cx).unwrap().theme, expected);
                view.shared =
                    Some(herdr_settings::Settings::parse_text("[theme]\nname='nord'").unwrap());
                choose(view, "Follow Herdr", cx);
            })
            .unwrap();
        source
            .update(cx, |view, _, cx| {
                view.settings.shared =
                    Some(herdr_settings::Settings::parse_text("[theme]\nname='dracula'").unwrap());
                view.apply_shared_theme(cx);
            })
            .unwrap();
        settings
    });
    cx.run_until_parked();
    cx.update(|cx| {
        let view = settings.read(cx).unwrap();
        assert_eq!(source.read(cx).unwrap().theme, view.theme);
        assert_eq!(
            cx.global::<crate::app::InitialAppearance>().theme,
            view.theme
        );
    });
}

pub(super) fn fixture() -> Loaded {
    Loaded {
        config: Config::default(),
        theme: Theme::default(),
        shared: None,
        error: None,
    }
}

fn open_fixture(source: WeakEntity<HerdrWindow>, cx: &mut App) {
    open_with(source, cx, |_, _, _| {});
}

#[gpui::test]
fn singleton_reactivation_and_close_preserve_source(cx: &mut TestAppContext) {
    let source = cx.add_window(crate::sidebar::layout_tests::fixture_window);
    cx.update(|cx| {
        let weak = source.update(cx, |_, _, cx| cx.weak_entity()).unwrap();
        let original = source
            .update(cx, |view, window, cx| (view.menu.page, window.focused(cx)))
            .unwrap();
        open_fixture(weak.clone(), cx);
        let first = cx.global::<SettingsWindowHandle>().window.unwrap();
        open_fixture(weak, cx);
        assert_eq!(cx.windows().len(), 2);
        assert_eq!(cx.global::<SettingsWindowHandle>().window.unwrap(), first);
        first
            .update(cx, |view, window, _| {
                assert_eq!(view.section, Section::Appearance);
                assert!(view.focus.is_focused(window));
                assert!(!view.busy());
                window.remove_window();
            })
            .unwrap();
        assert_eq!(cx.windows(), vec![source.into()]);
        source
            .update(cx, |view, window, cx| {
                assert_eq!(view.menu.page, original.0);
                assert_eq!(window.focused(cx), original.1);
            })
            .unwrap();
    });
}

#[gpui::test]
fn command_w_closes_only_settings(cx: &mut TestAppContext) {
    let source = cx.add_window(crate::sidebar::layout_tests::fixture_window);
    let weak = cx.update(|cx| source.update(cx, |_, _, cx| cx.weak_entity()).unwrap());
    let (view, cx) = cx.add_window_view(|window, cx| {
        cx.bind_keys(key_bindings());
        let mut view = SettingsWindow::new(weak, cx);
        view.theme_io = Some(themes::ThemeIo {
            write: Arc::new(|_, _| panic!("closing without a theme edit must not write")),
            load: Arc::new(|| panic!("closing without a theme edit must not reload")),
            resolve: None,
        });
        window.focus(&view.focus, cx);
        view
    });
    cx.update(|window, cx| {
        window.draw(cx).clear(cx);
        assert!(view.read(cx).focus.is_focused(window));
    });
    cx.simulate_keystrokes("cmd-w");
    assert_eq!(cx.windows(), vec![source.into()]);
}

#[gpui::test]
fn control_w_closes_only_settings(cx: &mut TestAppContext) {
    let source = cx.add_window(crate::sidebar::layout_tests::fixture_window);
    let weak = cx.update(|cx| source.update(cx, |_, _, cx| cx.weak_entity()).unwrap());
    let (_, cx) = cx.add_window_view(|window, cx| {
        crate::bind_keys(cx);
        let mut view = SettingsWindow::new(weak, cx);
        view.theme_io = Some(themes::ThemeIo {
            write: Arc::new(|_, _| panic!("closing without a theme edit must not write")),
            load: Arc::new(|| panic!("closing without a theme edit must not reload")),
            resolve: None,
        });
        window.focus(&view.focus, cx);
        view
    });
    cx.simulate_keystrokes("ctrl-w");
    assert_eq!(cx.windows(), vec![source.into()]);
}

#[gpui::test]
fn settings_shortcut_and_session_shortcuts_do_not_route_to_source(cx: &mut TestAppContext) {
    let source = cx.add_window(crate::sidebar::layout_tests::fixture_window);
    let weak = cx.update(|cx| source.update(cx, |_, _, cx| cx.weak_entity()).unwrap());
    let (view, cx) = cx.add_window_view(|window, cx| {
        crate::bind_keys(cx);
        let view = SettingsWindow::new(weak, cx);
        window.focus(&view.focus, cx);
        view
    });
    cx.simulate_keystrokes("cmd-,");
    cx.simulate_keystrokes("cmd-shift-w");
    cx.update(|_, cx| {
        assert_eq!(cx.windows().len(), 2);
        assert!(source.read(cx).unwrap().menu.page.is_none());
        assert!(view.read(cx).error.is_none());
        source
            .update(cx, |_, window, _| window.remove_window())
            .unwrap();
    });
    cx.simulate_keystrokes("cmd-,");
    assert_eq!(cx.windows().len(), 1);
    view.read_with(cx, |view, _| assert!(!view.busy()));
    cx.simulate_keystrokes("cmd-w");
    assert!(cx.windows().is_empty());
}

#[gpui::test]
fn explicit_open_retargets_after_source_closes_but_not_during_integration_work(
    cx: &mut TestAppContext,
) {
    let first = cx.add_window(crate::sidebar::layout_tests::fixture_window);
    let second = cx.add_window(crate::sidebar::layout_tests::fixture_window);
    let (settings, next) = cx.update(|cx| {
        let original = first.update(cx, |_, _, cx| cx.weak_entity()).unwrap();
        let next = second.update(cx, |_, _, cx| cx.weak_entity()).unwrap();
        open_fixture(original.clone(), cx);
        let settings = cx.global::<SettingsWindowHandle>().window.unwrap();
        first
            .update(cx, |source, _, _| source.integrations.busy = true)
            .unwrap();
        open_fixture(next.clone(), cx);
        assert_eq!(
            settings.read(cx).unwrap().source.entity_id(),
            original.entity_id()
        );
        first
            .update(cx, |_, window, _| window.remove_window())
            .unwrap();
        (settings, next)
    });
    cx.run_until_parked();
    cx.update(|cx| {
        open_fixture(next.clone(), cx);
        assert_eq!(
            cx.global::<SettingsWindowHandle>().window.unwrap(),
            settings
        );
        assert_eq!(
            settings.read(cx).unwrap().source.entity_id(),
            next.entity_id()
        );
        second
            .update(cx, |source, _, _| {
                assert!(!source.integrations.busy);
                assert!(source.menu.page.is_none());
            })
            .unwrap();
    });
}

#[gpui::test]
fn failed_load_keeps_validated_preferences_and_allows_retry(cx: &mut TestAppContext) {
    let source = cx.add_window(crate::sidebar::layout_tests::fixture_window);
    let settings = cx.update(|cx| {
        let weak = source.update(cx, |_, _, cx| cx.weak_entity()).unwrap();
        open_fixture(weak, cx);
        let settings = cx.global::<SettingsWindowHandle>().window.unwrap();
        settings
            .update(cx, |view, _, cx| {
                view.config.ui.size = 19.;
                view.reload_with(|| Err(crate::Error::MissingHome), cx);
                view.reload_with(|| panic!("loads must be serialized"), cx);
                assert!(view.busy());
            })
            .unwrap();
        settings
    });
    cx.run_until_parked();
    cx.update(|cx| {
        settings
            .update(cx, |view, _, cx| {
                assert_eq!(view.config.ui.size, 19.);
                assert!(
                    view.error
                        .as_ref()
                        .unwrap()
                        .contains("keeping current preferences")
                );
                assert!(!view.busy());
                view.reload_with(|| Ok(fixture()), cx);
            })
            .unwrap();
    });
    cx.run_until_parked();
    cx.update(|cx| {
        settings
            .update(cx, |view, _, _| {
                assert_eq!(view.config.ui.size, Config::default().ui.size);
                assert!(view.error.is_none());
            })
            .unwrap();
    });
}

#[gpui::test]
fn accepted_save_survives_both_windows_closing_and_reopen_reuses_model(cx: &mut TestAppContext) {
    let source = cx.add_window(crate::sidebar::layout_tests::fixture_window);
    let writes = Arc::new(AtomicUsize::new(0));
    let reconciliations = Arc::new(AtomicUsize::new(0));
    let retained = cx.update(|cx| {
        let weak = source.update(cx, |_, _, cx| cx.weak_entity()).unwrap();
        open_fixture(weak.clone(), cx);
        source
            .update(cx, |_, window, _| window.remove_window())
            .unwrap();
        let settings = cx.global::<SettingsWindowHandle>().window.unwrap();
        let writes = writes.clone();
        let reconciliations = reconciliations.clone();
        let retained = settings
            .update(cx, |view, window, cx| {
                view.save_with(
                    move || {
                        writes.fetch_add(1, Ordering::SeqCst);
                        Ok(())
                    },
                    move || {
                        reconciliations.fetch_add(1, Ordering::SeqCst);
                        Ok(fixture())
                    },
                    false,
                    cx,
                );
                view.save_with(
                    || panic!("writes must be serialized"),
                    || Ok(fixture()),
                    false,
                    cx,
                );
                assert!(view.saving);
                window.remove_window();
                cx.weak_entity()
            })
            .unwrap();
        open_fixture(weak, cx);
        let reopened = cx.global::<SettingsWindowHandle>().window.unwrap();
        reopened
            .update(cx, |view, window, cx| {
                assert_eq!(cx.entity_id(), retained.entity_id());
                assert!(view.saving);
                window.remove_window();
            })
            .unwrap();
        retained
    });
    cx.run_until_parked();
    assert_eq!(writes.load(Ordering::SeqCst), 1);
    assert_eq!(reconciliations.load(Ordering::SeqCst), 1);
    assert!(retained.upgrade().is_none());
    cx.update(|cx| assert!(cx.windows().is_empty()));
}

#[gpui::test]
fn categories_reset_scroll_without_touching_main_menu(cx: &mut TestAppContext) {
    let source = cx.add_window(crate::sidebar::layout_tests::fixture_window);
    cx.update(|cx| {
        let weak = source.update(cx, |_, _, cx| cx.weak_entity()).unwrap();
        open_fixture(weak, cx);
        let settings = cx.global::<SettingsWindowHandle>().window.unwrap();
        for section in Section::ALL {
            settings
                .update(cx, |view, window, cx| {
                    view.body_scroll.set_offset(point(px(0.), px(-100.)));
                    view.select_section(section, window, cx);
                    assert_eq!(view.section, section);
                    assert_eq!(view.body_scroll.offset(), Point::default());
                    assert!(view.focus.is_focused(window));
                })
                .unwrap();
            source
                .update(cx, |view, _, _| assert!(view.menu.page.is_none()))
                .unwrap();
        }
    });
}

#[gpui::test]
fn follow_appearance_resolves_prepared_settings_without_source_or_disk(cx: &mut TestAppContext) {
    let source = cx.add_window(crate::sidebar::layout_tests::fixture_window);
    let settings = cx.update(|cx| {
        let weak = source.update(cx, |_, _, cx| cx.weak_entity()).unwrap();
        open_fixture(weak, cx);
        source
            .update(cx, |_, window, _| window.remove_window())
            .unwrap();
        cx.global::<SettingsWindowHandle>().window.unwrap()
    });
    cx.run_until_parked();
    cx.update(|cx| {
        settings
            .update(cx, |view, _, cx| {
                let shared =
                    herdr_settings::Settings::parse_text("[theme]\nname = 'catppuccin'").unwrap();
                let light = matches!(
                    cx.window_appearance(),
                    WindowAppearance::Light | WindowAppearance::VibrantLight
                );
                let expected = shared
                    .theme(light)
                    .unwrap()
                    .with_contrast(crate::contrast::Contrast::High);
                let mut loaded = fixture();
                loaded.config.theme = "Follow Herdr".into();
                loaded.config.contrast = crate::contrast::Contrast::High;
                loaded.shared = Some(shared);
                view.apply_loaded(Ok(loaded), cx);
                assert_eq!(view.theme, expected);
                cx.set_global(crate::app::InitialAppearance::default());
                view.apply_window_appearance(cx);
                assert_eq!(cx.global::<crate::app::InitialAppearance>().theme, expected);
                assert_eq!(view.config.theme, "Follow Herdr");
                assert!(!view.busy());
                assert!(view.error.is_none());
                assert!(view.source.upgrade().is_none());
            })
            .unwrap();
    });
}

#[gpui::test]
fn sizes_accepted_behind_a_load_survive_closing_the_window(cx: &mut TestAppContext) {
    let source = cx.add_window(crate::sidebar::layout_tests::fixture_window);
    let writes = Arc::new(Mutex::new(Vec::new()));
    let weak = cx.update(|cx| {
        let source = source.update(cx, |_, _, cx| cx.weak_entity()).unwrap();
        open_fixture(source, cx);
        let settings = cx.global::<SettingsWindowHandle>().window.unwrap();
        settings
            .update(cx, |view, window, cx| {
                view.size_io = Some(recording_sizes(writes.clone()));
                view.reload_with(fixture_load, cx);
                view.accept_control_size(FontFace::Terminal, 19., cx);
                view.accept_control_size(FontFace::Terminal, 21., cx);
                view.accept_control_size(FontFace::Sidebar, 17., cx);
                assert!(view.loading);
                assert!(writes.lock().unwrap().is_empty());
                window.remove_window();
                cx.weak_entity()
            })
            .unwrap()
    });
    cx.run_until_parked();
    assert_eq!(
        *writes.lock().unwrap(),
        vec![vec![(FontFace::Terminal, 21.), (FontFace::Sidebar, 17.)]]
    );
    assert!(weak.upgrade().is_none());
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

#[gpui::test]
fn new_window_target_survives_source_close_and_tracks_live_source(cx: &mut TestAppContext) {
    use herdr_client::ConnectTarget;
    let source = cx.add_window(crate::sidebar::layout_tests::fixture_window);
    let original = ConnectTarget::Socket("/unused-original.sock".into());
    let current = ConnectTarget::Socket("/unused-current.sock".into());
    let settings = cx.update(|cx| {
        let weak = source
            .update(cx, |view, _, cx| {
                view.endpoints[0].connection.target = original.clone();
                cx.weak_entity()
            })
            .unwrap();
        open_fixture(weak, cx);
        let settings = cx.global::<SettingsWindowHandle>().window.unwrap();
        source
            .update(cx, |view, _, _| {
                view.endpoints[0].connection.target = current.clone()
            })
            .unwrap();
        assert_eq!(
            settings.read(cx).unwrap().additional_window_target(cx),
            Some(current)
        );
        source
            .update(cx, |_, window, _| window.remove_window())
            .unwrap();
        settings
    });
    cx.run_until_parked();
    cx.update(|cx| {
        assert_eq!(
            settings.read(cx).unwrap().additional_window_target(cx),
            Some(original)
        )
    });
}

#[gpui::test]
fn save_refresh_does_not_overwrite_main_theme_preview(cx: &mut TestAppContext) {
    let source = cx.add_window(crate::sidebar::layout_tests::fixture_window);
    let preview = Theme::builtin("Nord").unwrap();
    cx.update(|cx| {
        let weak = source
            .update(cx, |view, _, cx| {
                view.menu.page = Some(crate::menu::Page::Themes);
                view.theme = preview.clone();
                cx.weak_entity()
            })
            .unwrap();
        open_fixture(weak, cx);
        cx.global::<SettingsWindowHandle>()
            .window
            .unwrap()
            .update(cx, |view, _, cx| {
                view.save_with(|| Ok(()), fixture_load, false, cx);
            })
            .unwrap();
    });
    cx.run_until_parked();
    cx.update(|cx| {
        source
            .update(cx, |view, _, _| {
                assert_eq!(view.theme, preview);
                assert_eq!(view.menu.page, Some(crate::menu::Page::Themes));
                assert!(view.config_load.is_none());
                assert!(view.settings.task.is_none());
            })
            .unwrap()
    });
}

#[gpui::test]
fn reload_publishes_baseline_and_rebinds_only_changed_keys(cx: &mut TestAppContext) {
    let source = cx.add_window(crate::sidebar::layout_tests::fixture_window);
    cx.update(|cx| {
        cx.set_global(crate::app::InitialAppearance::default());
        cx.bind_keys([KeyBinding::new("ctrl-alt-z", Close, Some("SettingsWindow"))]);
        let weak = source.update(cx, |_, _, cx| cx.weak_entity()).unwrap();
        open_fixture(weak, cx);
        cx.global::<SettingsWindowHandle>()
            .window
            .unwrap()
            .update(cx, |view, _, cx| {
                let mut loaded = fixture();
                loaded.config.ui.size = 19.;
                loaded.config.theme = "Nord".into();
                loaded.theme = Theme::builtin("Nord").unwrap();
                view.apply_loaded(Ok(loaded), cx);
                assert_eq!(
                    cx.global::<crate::app::InitialAppearance>().config.ui.size,
                    19.
                );
                assert_eq!(
                    cx.global::<crate::app::InitialAppearance>().theme,
                    view.theme
                );
                let keys = cx.key_bindings();
                assert_eq!(keys.borrow().bindings_for_action(&Close).count(), 1);
                let mut loaded = fixture();
                loaded.config.keybindings = crate::keymap::Keymap::with_overrides(
                    &std::collections::BTreeMap::from([(
                        "settings".into(),
                        crate::keymap::Binding::One("ctrl-alt-p".into()),
                    )]),
                    &Default::default(),
                    &crate::keymap::DaemonKeys::default(),
                )
                .unwrap();
                view.apply_loaded(Ok(loaded), cx);
                assert_eq!(
                    cx.global::<crate::app::InitialAppearance>()
                        .config
                        .keybindings
                        .primary(crate::controls::Command::Settings),
                    "ctrl-alt-p"
                );
                assert_eq!(
                    keys.borrow().bindings_for_action(&Close).count(),
                    key_bindings().len()
                );
            })
            .unwrap();
    });
}
