use super::*;

/// Recovery reads A; another client then writes B before any reload. The save
/// must expect A, which conflicts on disk, rather than read B and replace it.
#[gpui::test]
fn a_recovered_shared_snapshot_is_the_saved_theme_expectation(cx: &mut TestAppContext) {
    // (quit instead of close, draft resolved before then)
    for (quit, resolved) in [(false, false), (true, true), (true, false)] {
        let main = cx.add_window(crate::sidebar::layout_tests::fixture_window);
        let loads = Arc::new(AtomicUsize::new(0));
        let writes = Arc::new(Mutex::new(Vec::new()));
        let settings = cx.update(|cx| {
            let weak = main.update(cx, |_, _, cx| cx.weak_entity()).unwrap();
            open_fixture(weak, cx);
            let settings = cx.global::<SettingsWindowHandle>().window.unwrap();
            settings
                .update(cx, |view, _, cx| {
                    assert!(view.shared.is_none());
                    let loads = loads.clone();
                    let writes = writes.clone();
                    view.theme_io = Some(themes::ThemeIo {
                        write: Arc::new(move |name, shared| {
                            let seen = shared.map(|shared| shared.sound_enabled);
                            writes.lock().unwrap().push((name, seen));
                            Ok(())
                        }),
                        load: Arc::new(fixture_load),
                        shared: Some(Arc::new(move || {
                            // A has sound on; every later read sees B, sound off.
                            let first = loads.fetch_add(1, Ordering::SeqCst) == 0;
                            let text = format!("[ui.sound]\nenabled = {first}\n");
                            Ok(herdr_settings::Settings::parse_text(&text)?)
                        })),
                        resolve: None,
                    });
                    view.accept_theme_choice(
                        themes::Choice {
                            scope: themes::Scope::Herdr,
                            name: "tokyo-night".into(),
                        },
                        cx,
                    );
                })
                .unwrap();
            settings
        });
        if resolved {
            cx.run_until_parked();
            assert_eq!(loads.load(Ordering::SeqCst), 1);
        }
        let task = cx.update(|cx| {
            settings
                .update(cx, |view, window, cx| {
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
        let writes = writes.lock().unwrap().clone();
        if quit && !resolved {
            // The draft is checked once more at quit, alongside the read already
            // in flight; the save is handed a snapshot rather than none.
            assert_eq!(writes.len(), 1, "quit before resolving");
            assert!(writes[0].1.is_some(), "quit before resolving");
        } else {
            assert_eq!(
                writes,
                [("tokyo-night".to_owned(), Some(true))],
                "quit: {quit}"
            );
            assert_eq!(loads.load(Ordering::SeqCst), 1, "quit: {quit}");
        }
        cx.update(|cx| {
            main.update(cx, |_, window, _| window.remove_window())
                .unwrap()
        });
    }
}
