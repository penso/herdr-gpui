use super::*;

pub(super) fn at(
    view: &Entity<SheprdWindow>,
    cx: &VisualTestContext,
    column: f32,
) -> Point<Pixels> {
    view.read_with(cx, |view, _| {
        view.bounds.origin
            + point(
                px(column * view.cell_width),
                px(view.config.terminal.line_height() + 1.),
            )
    })
}

pub(super) fn shift() -> Modifiers {
    Modifiers {
        shift: true,
        ..Default::default()
    }
}

pub(super) fn offer(view: &Entity<SheprdWindow>, cx: &mut VisualTestContext) {
    let start = at(view, cx, 0.);
    let end = at(view, cx, 6.);
    cx.simulate_mouse_down(start, MouseButton::Left, shift());
    cx.simulate_mouse_move(end, MouseButton::Left, shift());
    cx.simulate_mouse_up(end, MouseButton::Left, shift());
}

#[gpui::test]
fn shift_selection_offers_annotation_without_taking_focus_or_interrupting_copy(
    cx: &mut TestAppContext,
) {
    let (view, cx) = window(cx, shown(None, false));
    let start = at(&view, cx, 0.);
    let end = at(&view, cx, 6.);
    cx.simulate_mouse_down(start, MouseButton::Left, shift());
    cx.simulate_mouse_move(end, MouseButton::Left, shift());
    assert!(view.read_with(cx, |view, _| view.terminal_notes.offer.is_none()));
    cx.simulate_mouse_up(end, MouseButton::Left, shift());
    cx.update(|window, cx| {
        view.read_with(cx, |view, _| {
            assert!(view.terminal_notes.offer.is_some());
            assert!(!view.terminal_notes.composing());
            assert!(view.focus.is_focused(window));
        })
    });
    assert_eq!(
        cx.update(|_, cx| cx.read_from_clipboard().and_then(|item| item.text()))
            .as_deref(),
        Some("failed")
    );
    let pill = cx.debug_bounds("terminal-annotate").unwrap();
    cx.simulate_click(pill.center(), Modifiers::default());
    cx.update(|window, cx| {
        view.read_with(cx, |view, cx| {
            let composer = view.terminal_notes.composer.as_ref().unwrap();
            assert_eq!(composer.quote, "failed");
            assert_eq!(composer.pane_id, "w0:p1");
            assert!(view.terminal_notes.offer.is_none());
            assert!(view.terminal_note_focused(window, cx));
        })
    });
    cx.simulate_keystrokes("w h y");
    cx.simulate_keystrokes("enter");
    assert!(!view.read_with(cx, |view, _| view.terminal_notes.composing()));
    let copied = cx
        .update(|_, cx| cx.read_from_clipboard().and_then(|item| item.text()))
        .unwrap();
    assert!(copied.contains("   failed\n"));
    assert!(copied.ends_with("Note: why\n"));
}

#[gpui::test]
fn the_pill_works_with_mouse_reporting_and_each_copy_setting(cx: &mut TestAppContext) {
    let (view, cx) = window(cx, shown(None, false));
    for mouse_reporting in [false, true] {
        for copy_on_select in [false, true] {
            for keep in [false, true] {
                view.update(cx, |view, _| {
                    Arc::make_mut(view.live.surface.as_mut().unwrap()).panes[0].mouse_reporting =
                        mouse_reporting;
                    view.config.keep_selection_after_copy = keep;
                    view.settings.shared = Some(
                        crate::herdr_settings::Settings::parse_text(&format!(
                            "[ui]\ncopy_on_select = {copy_on_select}"
                        ))
                        .unwrap(),
                    );
                });
                cx.write_to_clipboard(gpui::ClipboardItem::new_string("before".into()));
                let start = at(&view, cx, 0.);
                let end = at(&view, cx, 6.);
                cx.simulate_mouse_down(start, MouseButton::Left, shift());
                cx.simulate_mouse_move(end, MouseButton::Left, Modifiers::default());
                cx.simulate_mouse_up(end, MouseButton::Left, Modifiers::default());
                cx.update(|window, cx| {
                    view.read_with(cx, |view, _| {
                        assert!(view.focus.is_focused(window));
                        assert!(view.terminal_notes.offer.is_some());
                        assert!(!view.terminal_notes.composing());
                        assert_eq!(view.selection_retained(), !copy_on_select || keep);
                    })
                });
                assert_eq!(
                    cx.update(|_, cx| cx.read_from_clipboard().and_then(|item| item.text()))
                        .as_deref(),
                    Some(if copy_on_select { "failed" } else { "before" })
                );
                let pill = cx.debug_bounds("terminal-annotate").unwrap();
                cx.simulate_click(pill.center(), Modifiers::default());
                assert!(view.read_with(cx, |view, _| view.terminal_notes.composing()));
                cx.simulate_keystrokes("escape");
                cx.update(|window, cx| {
                    view.read_with(cx, |view, _| {
                        assert!(!view.terminal_notes.composing());
                        assert!(view.focus.is_focused(window));
                    })
                });
            }
        }
    }
}

#[gpui::test]
fn plain_selection_shift_click_and_blanks_do_not_offer_annotation(cx: &mut TestAppContext) {
    let (view, cx) = window(cx, shown(None, false));
    cx.simulate_click(at(&view, cx, 0.), shift());
    assert!(view.read_with(cx, |view, _| view.terminal_notes.offer.is_none()));
    select(&view, cx);
    assert!(view.read_with(cx, |view, _| view.terminal_notes.offer.is_none()));
    let start = at(&view, cx, 18.);
    let end = at(&view, cx, 20.);
    cx.simulate_mouse_down(start, MouseButton::Left, shift());
    cx.simulate_mouse_move(end, MouseButton::Left, shift());
    cx.simulate_mouse_up(end, MouseButton::Left, shift());
    view.read_with(cx, |view, _| {
        assert!(!view.terminal_notes.composing());
        assert!(view.terminal_notes.offer.is_none());
    });
}

#[gpui::test]
fn copy_keys_keep_the_pill_but_typing_and_outside_clicks_dismiss_it(cx: &mut TestAppContext) {
    let (view, cx) = window(cx, shown(None, false));
    offer(&view, cx);
    cx.simulate_keystrokes("ctrl-shift-c");
    assert!(view.read_with(cx, |view, _| view.terminal_notes.offer.is_some()));
    assert_eq!(
        cx.update(|_, cx| cx.read_from_clipboard().and_then(|item| item.text()))
            .as_deref(),
        Some("failed")
    );
    cx.simulate_keystrokes("x");
    assert!(view.read_with(cx, |view, _| view.terminal_notes.offer.is_none()));
    offer(&view, cx);
    // Click within the terminal, not the native title bar: a window-relative
    // (5, 5) starts a window drag on macOS, unsupported by GPUI's test window.
    let outside = view.read_with(cx, |view, _| {
        view.bounds.origin
            + point(
                px(12. * view.cell_width),
                px(4. * view.config.terminal.line_height()),
            )
    });
    assert!(
        !cx.debug_bounds("terminal-annotate")
            .unwrap()
            .contains(&outside)
    );
    cx.simulate_click(outside, Modifiers::default());
    assert!(view.read_with(cx, |view, _| view.terminal_notes.offer.is_none()));
    assert!(!view.read_with(cx, |view, _| view.terminal_notes.composing()));
}
