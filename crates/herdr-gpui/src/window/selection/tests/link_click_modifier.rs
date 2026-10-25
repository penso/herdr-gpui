use super::*;
use crate::config::LinkClickModifier;
use gpui::{ClipboardItem, Entity, MouseDownEvent, Pixels, Point, VisualTestContext};

const URL: &str = "https://example.com/x";

/// A mouse-reporting pane showing `URL`, with links opened by any of
/// `modifiers`.
fn reporting_window<'a>(
    modifiers: &[&str],
    cx: &'a mut TestAppContext,
) -> (Entity<HerdrWindow>, &'a mut VisualTestContext) {
    let modifier = LinkClickModifier::parse_any(modifiers.iter().copied()).unwrap();
    let (view, cx) = cx.add_window_view(|window, cx| {
        let mut view = fixture_window(window, cx);
        let mut frame = surface(&[URL], 24);
        let snapshot = view.live.snapshot.as_ref().unwrap();
        frame.boot_id = snapshot.boot_id.clone();
        frame.projection_revision = snapshot.revision;
        frame.panes[0].mouse_reporting = true;
        view.live.surface = Some(Arc::new(frame));
        view.config.link_click_modifier = modifier;
        view
    });
    cx.update(|window, cx| {
        cx.write_to_clipboard(ClipboardItem::new_string("kept".into()));
        window.refresh();
        window.draw(cx).clear(cx);
    });
    (view, cx)
}

fn clipboard(cx: &mut VisualTestContext) -> Option<String> {
    cx.update(|_, cx| cx.read_from_clipboard().and_then(|item| item.text()))
}

const SHIFT: Modifiers = Modifiers {
    control: false,
    alt: false,
    shift: true,
    platform: false,
    function: false,
};

/// With no modifier, a plain press on a link is both a click that opens it
/// and the start of a selection: a drag selects, only a still release opens,
/// and a press anywhere else still goes to the application.
#[gpui::test]
fn without_a_modifier_a_plain_click_opens_and_a_drag_selects(cx: &mut TestAppContext) {
    let (view, cx) = reporting_window(&["none"], cx);
    let (origin, width) = view.read_with(cx, |view, _| (view.bounds.origin, view.cell_width));
    let at = |column: f32| origin + point(px(column * width), px(10.));

    cx.simulate_mouse_down(at(0.), MouseButton::Left, Modifiers::default());
    view.read_with(cx, |view, _| {
        assert!(view.selection.is_some());
        assert!(view.pressed_terminal_link.is_some());
    });
    cx.simulate_mouse_move(at(20.6), MouseButton::Left, Modifiers::default());
    cx.simulate_mouse_up(at(20.6), MouseButton::Left, Modifiers::default());
    assert!(cx.opened_url().is_none());
    assert_eq!(clipboard(cx), Some(URL.into()));

    // Past the link the press is the application's.
    cx.simulate_mouse_down(at(22.), MouseButton::Left, Modifiers::default());
    view.read_with(cx, |view, _| {
        assert!(view.selection.is_none());
        assert!(view.pressed_terminal_link.is_none());
    });
    cx.simulate_mouse_up(at(22.), MouseButton::Left, Modifiers::default());

    // The match is exact: the platform modifier or Shift opens nothing.
    cx.simulate_click(at(1.), Modifiers::secondary_key());
    cx.simulate_click(at(1.), SHIFT);
    assert!(cx.opened_url().is_none());
    // Alt only picks where the link opens.
    let alt = Modifiers {
        alt: true,
        ..Modifiers::default()
    };
    view.read_with(cx, |view, _| {
        assert!(view.terminal_link_press(at(1.), alt).is_some());
        assert!(view.terminal_link_hovered(at(1.), Modifiers::default()));
    });
    cx.simulate_click(at(1.), Modifiers::default());
    assert_eq!(cx.opened_url().as_deref(), Some(URL));
}

/// Shift as the modifier keeps Shift-drag selecting, as it does in a
/// mouse-reporting pane, while a Shift click that stays put opens.
#[gpui::test]
fn shift_as_the_modifier_still_drags_to_select(cx: &mut TestAppContext) {
    let (view, cx) = reporting_window(&["shift"], cx);
    let (origin, width) = view.read_with(cx, |view, _| (view.bounds.origin, view.cell_width));
    let at = |column: f32| origin + point(px(column * width), px(10.));

    cx.simulate_mouse_down(at(0.), MouseButton::Left, SHIFT);
    view.read_with(cx, |view, _| {
        assert!(view.selection.is_some());
        assert!(view.pressed_terminal_link.is_some());
    });
    cx.simulate_mouse_move(at(20.6), MouseButton::Left, SHIFT);
    cx.simulate_mouse_up(at(20.6), MouseButton::Left, SHIFT);
    assert!(cx.opened_url().is_none());
    assert_eq!(clipboard(cx), Some(URL.into()));

    cx.simulate_click(at(1.), Modifiers::default());
    cx.simulate_click(at(1.), Modifiers::secondary_key());
    assert!(cx.opened_url().is_none());
    cx.simulate_click(at(1.), SHIFT);
    assert_eq!(cx.opened_url().as_deref(), Some(URL));
}

const CMD: Modifiers = Modifiers {
    control: false,
    alt: false,
    shift: false,
    platform: true,
    function: false,
};

/// With both a plain click and Cmd allowed, either opens and underlines a
/// link, while Shift, in neither, still drags to select and opens nothing.
#[gpui::test]
fn a_list_of_modifiers_opens_with_any_and_shift_still_selects(cx: &mut TestAppContext) {
    let (view, cx) = reporting_window(&["none", "cmd"], cx);
    let (origin, width) = view.read_with(cx, |view, _| (view.bounds.origin, view.cell_width));
    let at = |column: f32| origin + point(px(column * width), px(10.));

    cx.simulate_mouse_down(at(0.), MouseButton::Left, SHIFT);
    view.read_with(cx, |view, _| {
        assert!(view.selection.is_some());
        assert!(view.pressed_terminal_link.is_none());
    });
    cx.simulate_mouse_move(at(20.6), MouseButton::Left, SHIFT);
    cx.simulate_mouse_up(at(20.6), MouseButton::Left, SHIFT);
    assert_eq!(clipboard(cx), Some(URL.into()));
    cx.simulate_click(at(1.), SHIFT);
    assert!(cx.opened_url().is_none());

    view.read_with(cx, |view, _| {
        for held in [Modifiers::default(), CMD] {
            assert!(view.terminal_link_press(at(1.), held).is_some(), "{held:?}");
            assert!(view.terminal_link_hovered(at(1.), held), "{held:?}");
        }
        assert!(view.terminal_link_press(at(1.), SHIFT).is_none());
        assert!(!view.terminal_link_hovered(at(1.), SHIFT));
    });
    cx.simulate_click(at(1.), CMD);
    assert_eq!(cx.opened_url().as_deref(), Some(URL));
}

/// Whether the window took a press of `button` at `position` for the pane's
/// application rather than leaving it to a link or the pane menu.
fn application_owns(
    view: &Entity<HerdrWindow>,
    position: Point<Pixels>,
    button: MouseButton,
    cx: &mut VisualTestContext,
) -> bool {
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            let press = MouseDownEvent {
                position,
                button,
                ..Default::default()
            };
            view.terminal_mouse_down(&press, window, cx)
        })
    })
}

/// Only a left press opens a link, so a plain middle or right press on one
/// in a mouse-reporting pane is the application's with "none" exactly as it
/// is with the default modifier: neither is dropped, and a right press in a
/// pane that routes right-clicks to the application opens no menu.
#[gpui::test]
fn without_a_modifier_middle_and_right_presses_on_a_link_still_reach_the_application(
    cx: &mut TestAppContext,
) {
    for setting in [None, Some(&["none"][..]), Some(&["none", "cmd"])] {
        let (view, cx) = reporting_window(setting.unwrap_or(&["none"]), cx);
        if setting.is_none() {
            view.update(cx, |view, _| {
                view.config.link_click_modifier = LinkClickModifier::default();
            });
        }
        let (origin, width) = view.read_with(cx, |view, _| (view.bounds.origin, view.cell_width));
        let on_link = origin + point(px(width), px(10.));
        assert!(
            application_owns(&view, on_link, MouseButton::Middle, cx),
            "{setting:?}"
        );
        // Unrouted, a right press opens the pane menu either way.
        assert!(
            !application_owns(&view, on_link, MouseButton::Right, cx),
            "{setting:?}"
        );
        // Herdr routes the drawn pane's right-clicks to the application.
        view.update(cx, |view, _| {
            let snapshot = Arc::make_mut(view.live.snapshot.as_mut().unwrap());
            snapshot.panes.push(ClientShellPane {
                pane_id: "pane".into(),
                workspace_id: String::new(),
                tab_id: String::new(),
                label: None,
                cwd: None,
                foreground_cwd: None,
                focused: true,
                right_click_passthrough: true,
            });
        });
        assert!(
            application_owns(&view, on_link, MouseButton::Right, cx),
            "{setting:?}"
        );
        cx.simulate_mouse_down(on_link, MouseButton::Right, Modifiers::default());
        view.read_with(cx, |view, _| {
            assert!(view.menu.page.is_none(), "{setting:?}");
            assert!(view.pressed_terminal_link.is_none(), "{setting:?}");
        });
        cx.simulate_mouse_up(on_link, MouseButton::Right, Modifiers::default());
    }
}
