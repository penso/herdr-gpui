use super::*;

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
