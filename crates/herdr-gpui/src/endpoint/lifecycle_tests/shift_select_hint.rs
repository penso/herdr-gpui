use super::*;
use crate::window::Flash;
use gpui::Modifiers;

fn hint() -> Flash {
    Flash::hint("Hold Shift and drag to select text")
}

fn hinted(view: &HerdrWindow) -> bool {
    view.flash
        .as_ref()
        .is_some_and(|(flash, _)| *flash == hint())
}

const SHIFT: Modifiers = Modifiers {
    shift: true,
    control: false,
    alt: false,
    platform: false,
    function: false,
};

/// Presses at column 3, moves to `to`, and releases there, as the window's
/// listeners would: a press the application does not take becomes a local
/// selection. Returns whether the application took the press.
fn drag(
    view: &mut HerdrWindow,
    button: MouseButton,
    modifiers: Modifiers,
    to: f32,
    window: &mut gpui::Window,
    cx: &mut Context<HerdrWindow>,
) -> bool {
    let down = MouseDownEvent {
        position: mouse_position(view, 3.5, 4.5),
        button,
        modifiers,
        click_count: 1,
        ..Default::default()
    };
    let to = mouse_position(view, to, 4.5);
    let moved = MouseMoveEvent {
        position: to,
        pressed_button: Some(button),
        modifiers,
    };
    let up = MouseUpEvent {
        position: to,
        button,
        modifiers,
        click_count: 1,
    };
    if view.terminal_mouse_down(&down, window, cx) {
        assert!(view.terminal_mouse_move(&moved, cx));
        assert!(view.terminal_mouse_up(&up, cx));
        return true;
    }
    view.begin_selection(down.position, 1, modifiers.shift, cx);
    view.extend_selection(to, cx);
    view.release_selection(cx);
    false
}

/// A drag the application took is where copying looks broken: the hint
/// answers it once per session, and a second drag is left alone.
#[gpui::test]
fn a_forwarded_drag_hints_shift_once_per_session(cx: &mut gpui::TestAppContext) {
    let (fixture, cx) = cx.add_window_view(|window, cx| {
        Fixture(cx.new(|cx| crate::sidebar::layout_tests::fixture_window(window, cx)))
    });
    let view = fixture.update(cx, |fixture, _| fixture.0.clone());
    let (endpoint, _server) = connected_endpoint("ssh:mouse");
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            prepare_mouse(view, endpoint, cx);
            assert!(view.flash.is_none());
            assert!(drag(
                view,
                MouseButton::Left,
                Modifiers::default(),
                9.5,
                window,
                cx
            ));
            assert!(hinted(view), "{:?}", view.flash);
            // It teaches, so it outlasts a two-second confirmation.
            assert!(!view.tick_flash(Instant::now() + Duration::from_secs(3)));
            assert!(view.tick_flash(Instant::now() + Duration::from_secs(5)));

            assert!(drag(
                view,
                MouseButton::Left,
                Modifiers::default(),
                12.5,
                window,
                cx
            ));
            assert!(view.flash.is_none(), "{:?}", view.flash);
        });
    });
}

/// A click, a press that never reached another cell, and a middle-button
/// drag are not selection attempts: none of them spends the hint, which the
/// next left drag still gets.
#[gpui::test]
fn clicks_and_other_buttons_leave_the_hint_unspent(cx: &mut gpui::TestAppContext) {
    let (fixture, cx) = cx.add_window_view(|window, cx| {
        Fixture(cx.new(|cx| crate::sidebar::layout_tests::fixture_window(window, cx)))
    });
    let view = fixture.update(cx, |fixture, _| fixture.0.clone());
    let (endpoint, _server) = connected_endpoint("ssh:mouse");
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            prepare_mouse(view, endpoint, cx);
            for (button, to) in [
                (MouseButton::Left, 3.5),
                (MouseButton::Left, 3.9),
                (MouseButton::Middle, 9.5),
            ] {
                assert!(drag(view, button, Modifiers::default(), to, window, cx));
                assert!(view.flash.is_none(), "{button:?} to {to}: {:?}", view.flash);
            }
            assert!(drag(
                view,
                MouseButton::Left,
                Modifiers::default(),
                9.5,
                window,
                cx
            ));
            assert!(hinted(view), "{:?}", view.flash);
        });
    });
}

/// A pane whose program ignores the mouse selects locally, so there is
/// nothing to hint, and that selection does not spend the hint either.
#[gpui::test]
fn a_local_selection_in_a_pane_without_mouse_reporting_never_hints(cx: &mut gpui::TestAppContext) {
    let (fixture, cx) = cx.add_window_view(|window, cx| {
        Fixture(cx.new(|cx| crate::sidebar::layout_tests::fixture_window(window, cx)))
    });
    let view = fixture.update(cx, |fixture, _| fixture.0.clone());
    let (endpoint, _server) = connected_endpoint("ssh:mouse");
    let reporting = |view: &mut HerdrWindow, on: bool| {
        for pane in &mut Arc::make_mut(view.live.surface.as_mut().unwrap()).panes {
            pane.mouse_reporting = on;
        }
    };
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            prepare_mouse(view, endpoint, cx);
            reporting(view, false);
            for modifiers in [Modifiers::default(), SHIFT] {
                assert!(!drag(view, MouseButton::Left, modifiers, 9.5, window, cx));
                assert!(!hinted(view), "{modifiers:?}: {:?}", view.flash);
            }
            view.flash = None;
            reporting(view, true);
            assert!(drag(
                view,
                MouseButton::Left,
                Modifiers::default(),
                9.5,
                window,
                cx
            ));
            assert!(hinted(view), "{:?}", view.flash);
        });
    });
}

/// Shift over a mouse-reporting pane keeps the drag local and selects. The
/// user knows the gesture, so a later forwarded drag shows no hint.
#[gpui::test]
fn a_shift_selection_over_a_mouse_reporting_pane_retires_the_hint(cx: &mut gpui::TestAppContext) {
    let (fixture, cx) = cx.add_window_view(|window, cx| {
        Fixture(cx.new(|cx| crate::sidebar::layout_tests::fixture_window(window, cx)))
    });
    let view = fixture.update(cx, |fixture, _| fixture.0.clone());
    let (endpoint, _server) = connected_endpoint("ssh:mouse");
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            prepare_mouse(view, endpoint, cx);
            assert!(!drag(view, MouseButton::Left, SHIFT, 9.5, window, cx));
            assert!(view.terminal_mouse.is_none());
            assert!(!hinted(view), "{:?}", view.flash);
            view.flash = None;
            assert!(drag(
                view,
                MouseButton::Left,
                Modifiers::default(),
                9.5,
                window,
                cx
            ));
            assert!(view.flash.is_none(), "{:?}", view.flash);
        });
    });
}

/// An application that asked for pixels is sent motion within one cell,
/// but that press selected nothing either: only reaching another cell
/// spends the hint.
#[gpui::test]
fn pixel_motion_within_one_cell_leaves_the_hint_unspent(cx: &mut gpui::TestAppContext) {
    let (fixture, cx) = cx.add_window_view(|window, cx| {
        Fixture(cx.new(|cx| crate::sidebar::layout_tests::fixture_window(window, cx)))
    });
    let view = fixture.update(cx, |fixture, _| fixture.0.clone());
    let (endpoint, _server) = connected_endpoint("ssh:mouse");
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            prepare_mouse(view, endpoint, cx);
            for pane in &mut Arc::make_mut(view.live.surface.as_mut().unwrap()).panes {
                pane.sgr_pixel_mouse = true;
            }
            assert!(drag(
                view,
                MouseButton::Left,
                Modifiers::default(),
                3.9,
                window,
                cx
            ));
            assert!(view.flash.is_none(), "{:?}", view.flash);
            assert!(drag(
                view,
                MouseButton::Left,
                Modifiers::default(),
                9.5,
                window,
                cx
            ));
            assert!(hinted(view), "{:?}", view.flash);
        });
    });
}
