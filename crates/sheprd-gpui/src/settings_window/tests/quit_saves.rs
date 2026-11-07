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
                        shared: None,
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
                        shared: None,
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
fn quit_writes_a_live_sidebar_draft_even_behind_its_own_running_save(cx: &mut TestAppContext) {
    use crate::config::SidebarOverrides;
    use crate::config::preferences::Preference;
    #[derive(Clone, Copy, Debug, PartialEq)]
    enum Case {
        Idle,
        InFlight,
        /// A writes indent 10 and a host colour; the draft then goes back to
        /// the loaded values before quit.
        InFlightThenRevert,
    }
    for case in [Case::Idle, Case::InFlight, Case::InFlightThenRevert] {
        let edits = Arc::new(Mutex::new(Vec::new()));
        let captured = edits.clone();
        let (view, cx) = cx.add_window_view(skill_fixture);
        let task = view.update(cx, |view, cx| {
            view.controls.preference_io = Some(controls::preferences::PreferenceIo::recording(
                move |edit| captured.lock().unwrap().push(edit),
            ));
            let base = SidebarIntent::of(&view.config);
            let mut draft = base.clone();
            draft.style.overrides = SidebarOverrides {
                indent: Some(10.),
                ..Default::default()
            };
            if case != Case::Idle {
                if case == Case::InFlightThenRevert {
                    draft.style.hosts.insert("Box".into(), 0x112233);
                }
                view.commit_sidebar(draft.clone(), cx);
                assert!(view.busy());
            }
            if case == Case::InFlightThenRevert {
                view.preview_sidebar(base.clone(), cx);
            } else {
                draft.style.overrides.row_padding = Some(2.);
                view.preview_sidebar(draft, cx);
            }
            view.shutdown(cx)
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
        result.try_recv().unwrap().unwrap();
        // The quit writes every key the draft touched, so a value an
        // in-flight save changed is put back even when the draft equals the
        // last load; keys it never touched are left as the file has them.
        let quit = |indent, row_padding| {
            vec![
                Preference::SidebarIndent(indent),
                Preference::SidebarRowPadding(row_padding),
            ]
        };
        let expected = match case {
            Case::Idle => quit(Some(10.), Some(2.)),
            Case::InFlight => {
                let mut expected = vec![Preference::SidebarIndent(Some(10.))];
                expected.extend(quit(Some(10.), Some(2.)));
                expected
            }
            Case::InFlightThenRevert => vec![
                Preference::SidebarIndent(Some(10.)),
                Preference::HostColor {
                    host: "Box".into(),
                    color: Some(0x112233),
                },
                Preference::SidebarIndent(None),
                Preference::HostColor {
                    host: "Box".into(),
                    color: None,
                },
            ],
        };
        assert_eq!(*edits.lock().unwrap(), expected, "{case:?}");
    }
}
