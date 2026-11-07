use super::*;
use crate::config::{Backdrop, backdrop::Edit};
use crate::settings_window::controls::backdrop::{Mode, fraction, opacity_at};

fn backdrop(opacity: f32, image: Option<&str>) -> Backdrop {
    Backdrop {
        opacity,
        blur: false,
        image: image.map(Into::into),
        ..Backdrop::default()
    }
}

#[test]
fn the_mode_follows_the_config_and_switching_writes_only_what_differs() {
    let solid = backdrop(1., None);
    let translucent = backdrop(0.7, None);
    let picture = backdrop(0.7, Some("/pictures/sea.jpg"));
    assert_eq!(Mode::of(&solid), Mode::Solid);
    assert_eq!(Mode::of(&translucent), Mode::Translucent);
    assert_eq!(Mode::of(&picture), Mode::Picture);
    // A picture under a solid background is still the picture mode.
    assert_eq!(Mode::of(&backdrop(1., Some("/a.png"))), Mode::Picture);

    assert_eq!(Mode::Translucent.edits(&solid), [Edit::Opacity(0.85)]);
    // Leaving a picture keeps the opacity it was shown at.
    assert_eq!(Mode::Translucent.edits(&picture), [Edit::Image(None)]);
    assert_eq!(Mode::Solid.edits(&translucent), [Edit::Opacity(1.)]);
    assert_eq!(
        Mode::Solid.edits(&picture),
        [Edit::Image(None), Edit::Opacity(1.)]
    );
    // Picture needs a file, which only the chooser gives.
    assert!(Mode::Picture.edits(&solid).is_empty());
}

#[test]
fn the_ruler_maps_whole_percents_from_ten_to_a_hundred() {
    assert_eq!(opacity_at(0.), 0.1);
    assert_eq!(opacity_at(1.), 1.);
    assert_eq!(opacity_at(0.5), 0.55);
    // 95% is one step from the end, not a guess between marks.
    assert_eq!(opacity_at(85. / 90.), 0.95);
    assert_eq!(opacity_at(-3.), 0.1);
    assert_eq!(opacity_at(7.), 1.);
    for percent in 10..=100 {
        let opacity = percent as f32 / 100.;
        assert_eq!(opacity_at(fraction(opacity)), opacity, "{percent}%");
    }
    // A value from the file outside the ruler pins to its end.
    assert_eq!(fraction(0.05), 0.);
    assert_eq!(fraction(1.5), 1.);
}

#[gpui::test]
fn the_ruler_follows_a_drag_and_writes_nothing_until_release(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(skill_fixture);
    cx.simulate_resize(size(px(960.), px(3200.)));
    view.update(cx, |view, cx| {
        view.section = Section::Theme;
        view.config.backdrop = backdrop(0.85, None);
        cx.notify();
    });
    let draw = |cx: &mut VisualTestContext| {
        cx.update(|window, cx| crate::sidebar::layout_tests::full_draw(window, cx).clear(cx));
    };
    draw(cx);
    let track = cx.debug_bounds("settings-backdrop-ruler").unwrap();
    let knob_x = |cx: &mut VisualTestContext| {
        cx.debug_bounds("settings-backdrop-knob")
            .unwrap()
            .center()
            .x
    };
    let at = |fraction: f32| point(track.left() + track.size.width * fraction, track.center().y);
    // 85% sits 75/90 along the track.
    assert!((knob_x(cx) - at(75. / 90.).x).abs() < px(1.));

    // A press lands at once, then the knob follows the pointer.
    cx.simulate_mouse_down(at(0.5), MouseButton::Left, Modifiers::default());
    draw(cx);
    assert_eq!(
        view.read_with(cx, |view, _| view.controls.ruler_held()),
        Some(0.55)
    );
    assert!((knob_x(cx) - at(0.5).x).abs() < px(1.));
    cx.simulate_mouse_move(at(0.25), MouseButton::Left, Modifiers::default());
    cx.simulate_mouse_move(at(0.), MouseButton::Left, Modifiers::default());
    draw(cx);
    assert_eq!(
        view.read_with(cx, |view, _| view.controls.ruler_held()),
        Some(0.1)
    );
    assert!((knob_x(cx) - at(0.).x).abs() < px(1.));
    // The config is untouched while the button is down.
    assert_eq!(
        view.read_with(cx, |view, _| view.config.backdrop.opacity),
        0.85
    );

    // Released while a save runs, the value is dropped, not written, and
    // the knob returns to the saved value. A release while idle writes the
    // user's configuration, which tests leave alone.
    view.update(cx, |view, _| view.saving = true);
    cx.simulate_mouse_up(at(0.), MouseButton::Left, Modifiers::default());
    draw(cx);
    view.read_with(cx, |view, _| {
        assert_eq!(view.controls.ruler_held(), None);
        assert_eq!(view.config.backdrop.opacity, 0.85);
    });
    assert!((knob_x(cx) - at(75. / 90.).x).abs() < px(1.));
    for mark in [
        "settings-backdrop-mark-10",
        "settings-backdrop-mark-25",
        "settings-backdrop-mark-50",
        "settings-backdrop-mark-75",
        "settings-backdrop-mark-100",
    ] {
        assert!(cx.debug_bounds(mark).is_some(), "{mark}");
    }
}

#[gpui::test]
fn a_write_shows_its_value_only_while_it_runs(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(skill_fixture);
    cx.simulate_resize(size(px(960.), px(3200.)));
    view.update(cx, |view, cx| {
        view.section = Section::Theme;
        view.config.backdrop = backdrop(0.85, None);
        view.saving = true;
        view.controls.set_ruler_saving(Some(0.1));
        cx.notify();
    });
    let draw = |cx: &mut VisualTestContext| {
        cx.update(|window, cx| crate::sidebar::layout_tests::full_draw(window, cx).clear(cx));
    };
    draw(cx);
    let track = cx.debug_bounds("settings-backdrop-ruler").unwrap();
    let knob_x = |cx: &mut VisualTestContext| {
        cx.debug_bounds("settings-backdrop-knob")
            .unwrap()
            .center()
            .x
    };
    // While the write runs, the knob shows the value being written.
    assert!((knob_x(cx) - track.left()).abs() < px(1.));
    // Once it ends, failed or not, the config's value shows again: the
    // config was never changed ahead of the write.
    view.update(cx, |view, cx| {
        view.saving = false;
        cx.notify();
    });
    draw(cx);
    let saved = track.left() + track.size.width * (75. / 90.);
    assert!((knob_x(cx) - saved).abs() < px(1.));
    assert_eq!(
        view.read_with(cx, |view, _| view.config.backdrop.opacity),
        0.85
    );
}

#[gpui::test]
fn each_mode_shows_three_tiles_and_only_its_own_controls(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(skill_fixture);
    cx.simulate_resize(size(px(960.), px(3200.)));
    for (config, stepper, blur, picture) in [
        (backdrop(1., None), false, false, false),
        (backdrop(0.8, None), true, true, false),
        (backdrop(0.75, Some("/pictures/sea.jpg")), true, false, true),
    ] {
        view.update(cx, |view, cx| {
            view.section = Section::Theme;
            view.config.backdrop = config;
            cx.notify();
        });
        cx.update(|window, cx| crate::sidebar::layout_tests::full_draw(window, cx).clear(cx));
        for tile in [
            "settings-backdrop-solid",
            "settings-backdrop-translucent",
            "settings-backdrop-picture",
        ] {
            assert!(cx.debug_bounds(tile).is_some(), "{tile}");
        }
        assert_eq!(
            cx.debug_bounds("settings-backdrop-opacity").is_some(),
            stepper
        );
        assert_eq!(cx.debug_bounds("settings-backdrop-blur").is_some(), blur);
        assert_eq!(
            cx.debug_bounds("settings-backdrop-remove").is_some(),
            picture
        );
        // The tiles share one row and stay inside the card.
        let card = cx.debug_bounds("settings-backdrop").unwrap();
        let solid = cx.debug_bounds("settings-backdrop-solid").unwrap();
        let tile = cx.debug_bounds("settings-backdrop-picture").unwrap();
        assert_eq!(solid.top(), tile.top());
        assert!(tile.right() <= card.right());
    }
}
