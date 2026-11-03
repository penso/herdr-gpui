use super::*;
use crate::settings_window::{controls::preferences::PreferenceIo, inputs::NumberField};
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

fn quiet_load() -> crate::Result<super::super::super::Loaded> {
    let mut loaded = skill_load()?;
    loaded.config.sound.volume = 25;
    Ok(loaded)
}

/// A volume chosen in Settings applies to the next sound once its save
/// loads, without waiting for a main window to reload the file.
#[gpui::test]
fn a_saved_volume_reaches_the_shared_sound_worker_at_once(cx: &mut TestAppContext) {
    // The main window must outlive the fixture, as a real one does.
    let source = cx.add_window(crate::sidebar::layout_tests::fixture_window);
    let weak = cx.update(|cx| source.update(cx, |_, _, cx| cx.weak_entity()).unwrap());
    let (view, cx) = cx.add_window_view(move |_, cx| SettingsWindow::new(weak.clone(), cx));
    let (sound, volumes) = crate::sound::Service::recording_volume();
    let sound = std::rc::Rc::new(sound);
    let edits = Arc::new(Mutex::new(Vec::new()));
    let captured = edits.clone();
    cx.update(|_, cx| {
        source
            .update(cx, |source, _, _| source.use_sound(sound.clone()))
            .unwrap();
    });
    view.update(cx, |view, cx| {
        view.controls.preference_io = Some(PreferenceIo {
            write: Arc::new(move |edit| {
                captured.lock().unwrap().push(edit);
                Ok(())
            }),
            load: Arc::new(quiet_load),
        });
        view.section = Section::Notifications;
        cx.notify();
    });
    cx.simulate_resize(size(px(960.), px(3200.)));
    cx.update(|window, cx| crate::sidebar::layout_tests::full_draw(window, cx).clear(cx));
    // Type 25 into the volume field and press Enter.
    let field = cx.debug_bounds("notif-volume-control-field").unwrap();
    assert!(field.size.width > px(0.));
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.begin_number_edit(NumberField::SoundVolume, 50., window, cx)
        })
    });
    cx.simulate_input("25");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();

    assert_eq!(
        edits.lock().unwrap().as_slice(),
        [crate::config::preferences::Preference::SoundVolume(25)]
    );
    // The main window's own config still holds the old volume.
    cx.update(|_, cx| {
        let volume = source.read_with(cx, |source, _| source.config.sound.volume);
        assert_eq!(volume.unwrap(), 50);
    });
    sound.preview();
    assert_eq!(volumes.recv_timeout(Duration::from_secs(3)).unwrap(), 25);
}

/// Dragging the volume slider moves its fill and number at once; only the
/// release writes, once.
#[gpui::test]
fn a_volume_drag_shows_its_value_before_release_and_writes_once(cx: &mut TestAppContext) {
    let source = cx.add_window(crate::sidebar::layout_tests::fixture_window);
    let weak = cx.update(|cx| source.update(cx, |_, _, cx| cx.weak_entity()).unwrap());
    let (view, cx) = cx.add_window_view(move |_, cx| SettingsWindow::new(weak.clone(), cx));
    let edits = Arc::new(Mutex::new(Vec::new()));
    let captured = edits.clone();
    // The reload reflects the written volume, as the file would.
    let disk = Arc::new(Mutex::new(None));
    let written = disk.clone();
    view.update(cx, |view, cx| {
        view.controls.preference_io = Some(PreferenceIo {
            write: Arc::new(move |edit| {
                if let crate::config::preferences::Preference::SoundVolume(volume) = edit {
                    *written.lock().unwrap() = Some(volume);
                }
                captured.lock().unwrap().push(edit);
                Ok(())
            }),
            load: Arc::new(move || {
                let mut loaded = quiet_load()?;
                if let Some(volume) = *disk.lock().unwrap() {
                    loaded.config.sound.volume = volume;
                }
                Ok(loaded)
            }),
        });
        view.section = Section::Notifications;
        cx.notify();
    });
    cx.simulate_resize(size(px(960.), px(3200.)));
    cx.update(|window, cx| crate::sidebar::layout_tests::full_draw(window, cx).clear(cx));
    let saved = view.read_with(cx, |view, _| f32::from(view.config.sound.volume));
    let shown = |cx: &mut VisualTestContext| {
        view.read_with(cx, |view, _| {
            view.number_value(
                NumberField::SoundVolume,
                f32::from(view.config.sound.volume),
            )
        })
    };
    assert_eq!(shown(cx), saved);
    let slider = cx.debug_bounds("notif-volume-control-slider").unwrap();
    let start = point(slider.left() + px(1.), slider.center().y);
    let end = point(slider.right() - px(1.), slider.center().y);
    cx.simulate_mouse_down(start, MouseButton::Left, Default::default());
    // The first move past the threshold only starts the drag; the next one moves it.
    cx.simulate_mouse_move(
        point(start.x + px(20.), start.y),
        MouseButton::Left,
        Default::default(),
    );
    cx.simulate_mouse_move(end, MouseButton::Left, Default::default());
    cx.run_until_parked();
    let dragged = shown(cx);
    assert!(dragged > saved, "the number follows the drag: {dragged}");
    assert_eq!(
        view.read_with(cx, |view, _| f32::from(view.config.sound.volume)),
        saved,
        "the config waits for the release"
    );
    assert!(edits.lock().unwrap().is_empty(), "no write while dragging");
    cx.simulate_mouse_up(end, MouseButton::Left, Default::default());
    assert_eq!(shown(cx), dragged, "the released value shows while saving");
    cx.run_until_parked();
    assert_eq!(
        edits.lock().unwrap().as_slice(),
        [crate::config::preferences::Preference::SoundVolume(
            dragged as u8
        )]
    );
    // The released value stays on screen while the save runs, and the
    // reload that ends it shows the same value.
    let released = view.read_with(cx, |view, _| view.number_preview.is_none());
    assert!(released, "the reload ends the preview");
    assert_eq!(shown(cx), dragged, "the released value stays");
}

/// A drag whose page goes away before release leaves no value behind:
/// coming back shows the saved volume, and nothing was written.
#[gpui::test]
fn a_volume_drag_abandoned_by_leaving_the_page_shows_the_saved_value(cx: &mut TestAppContext) {
    let source = cx.add_window(crate::sidebar::layout_tests::fixture_window);
    let weak = cx.update(|cx| source.update(cx, |_, _, cx| cx.weak_entity()).unwrap());
    let (view, cx) = cx.add_window_view(move |_, cx| SettingsWindow::new(weak.clone(), cx));
    let edits = Arc::new(Mutex::new(Vec::new()));
    let captured = edits.clone();
    view.update(cx, |view, cx| {
        view.controls.preference_io = Some(PreferenceIo {
            write: Arc::new(move |edit| {
                captured.lock().unwrap().push(edit);
                Ok(())
            }),
            load: Arc::new(quiet_load),
        });
        view.section = Section::Notifications;
        cx.notify();
    });
    cx.simulate_resize(size(px(960.), px(3200.)));
    cx.update(|window, cx| crate::sidebar::layout_tests::full_draw(window, cx).clear(cx));
    let saved = view.read_with(cx, |view, _| f32::from(view.config.sound.volume));
    let shown = |cx: &mut VisualTestContext| {
        view.read_with(cx, |view, _| {
            view.number_value(
                NumberField::SoundVolume,
                f32::from(view.config.sound.volume),
            )
        })
    };
    let slider = cx.debug_bounds("notif-volume-control-slider").unwrap();
    let start = point(slider.left() + px(1.), slider.center().y);
    let end = point(slider.right() - px(1.), slider.center().y);
    cx.simulate_mouse_down(start, MouseButton::Left, Default::default());
    cx.simulate_mouse_move(
        point(start.x + px(20.), start.y),
        MouseButton::Left,
        Default::default(),
    );
    cx.simulate_mouse_move(end, MouseButton::Left, Default::default());
    cx.run_until_parked();
    assert_ne!(shown(cx), saved, "the drag is previewed");
    // Leave the page mid-drag, then come back, without releasing.
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.select_section(Section::General, window, cx);
            view.select_section(Section::Notifications, window, cx);
        })
    });
    cx.update(|window, cx| crate::sidebar::layout_tests::full_draw(window, cx).clear(cx));
    cx.run_until_parked();
    assert_eq!(shown(cx), saved, "the saved volume shows again");
    assert!(view.read_with(cx, |view, _| view.number_preview.is_none()));
    assert!(edits.lock().unwrap().is_empty(), "nothing was written");
}
