use super::*;
use crate::config::{Axis, Padding, Sides, preferences::Preference};
use crate::settings_window::controls::preferences::PreferenceIo;
use std::sync::{Arc, Mutex};

#[gpui::test]
fn each_side_steps_alone_within_the_band(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(skill_fixture);
    let edits = Arc::new(Mutex::new(Vec::new()));
    let captured = edits.clone();
    view.update(cx, |view, _| {
        view.controls.preference_io = Some(PreferenceIo {
            write: Arc::new(move |edit| {
                captured.lock().unwrap().push(edit);
                Ok(())
            }),
            load: Arc::new(skill_load),
        });
    });
    cx.simulate_resize(size(px(960.), px(2600.)));
    cx.update(|window, cx| crate::sidebar::layout_tests::full_draw(window, cx).clear(cx));
    // The card belongs to Appearance, not to the section shown first.
    assert!(cx.debug_bounds("settings-terminal-padding").is_none());
    assert!(cx.debug_bounds("terminal-padding-left-plus").is_none());

    let sides = |start, end| Sides::new(start, end).unwrap();
    let (x, y) = (sides(8., 2.), sides(3., 5.));
    for (x, y, button, edit) in [
        // A step moves its own side and keeps the other side of the axis.
        (x, y, "terminal-padding-left-plus", (Axis::X, sides(9., 2.))),
        (
            x,
            y,
            "terminal-padding-left-minus",
            (Axis::X, sides(7., 2.)),
        ),
        (
            x,
            y,
            "terminal-padding-right-plus",
            (Axis::X, sides(8., 3.)),
        ),
        (
            x,
            y,
            "terminal-padding-right-minus",
            (Axis::X, sides(8., 1.)),
        ),
        (x, y, "terminal-padding-top-plus", (Axis::Y, sides(4., 5.))),
        (x, y, "terminal-padding-top-minus", (Axis::Y, sides(2., 5.))),
        (
            x,
            y,
            "terminal-padding-bottom-plus",
            (Axis::Y, sides(3., 6.)),
        ),
        (
            x,
            y,
            "terminal-padding-bottom-minus",
            (Axis::Y, sides(3., 4.)),
        ),
        // The band's ends hold.
        (
            sides(0., 64.),
            y,
            "terminal-padding-left-minus",
            (Axis::X, sides(0., 64.)),
        ),
        (
            sides(0., 64.),
            y,
            "terminal-padding-right-plus",
            (Axis::X, sides(0., 64.)),
        ),
        (
            x,
            sides(64., 0.),
            "terminal-padding-top-plus",
            (Axis::Y, sides(64., 0.)),
        ),
        (
            x,
            sides(64., 0.),
            "terminal-padding-bottom-minus",
            (Axis::Y, sides(64., 0.)),
        ),
    ] {
        view.update(cx, |view, cx| {
            view.section = Section::Theme;
            view.config.terminal_padding = Padding { x, y };
            cx.notify();
        });
        cx.update(|window, cx| crate::sidebar::layout_tests::full_draw(window, cx).clear(cx));
        assert!(cx.debug_bounds("settings-terminal-padding").is_some());
        let point = cx.debug_bounds(button).unwrap().center();
        cx.simulate_click(point, Default::default());
        cx.run_until_parked();
        assert_eq!(
            edits.lock().unwrap().pop(),
            Some(Preference::TerminalPadding(edit.0, edit.1)),
            "{button} from {x:?} {y:?}"
        );
    }
}

/// The sections under the theme grid keep one distance from what is above.
#[gpui::test]
fn the_appearance_cards_are_evenly_spaced(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(skill_fixture);
    cx.simulate_resize(size(px(960.), px(2600.)));
    view.update(cx, |view, cx| {
        view.section = Section::Theme;
        cx.notify();
    });
    cx.update(|window, cx| crate::sidebar::layout_tests::full_draw(window, cx).clear(cx));
    let grid = cx.debug_bounds("settings-theme-grid").unwrap();
    let window = cx.debug_bounds("settings-window-section").unwrap();
    let padding = cx.debug_bounds("settings-terminal-padding").unwrap();
    let backdrop = cx.debug_bounds("settings-backdrop").unwrap();
    let gap = window.top() - grid.bottom();
    assert!(gap > px(0.));
    assert_eq!(padding.top() - backdrop.bottom(), gap);
}
