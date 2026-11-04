use super::*;

/// Cmd-A and Edit > Select All take the focused pane's whole screen as a
/// kept highlight without touching the clipboard; Cmd-C then copies it, and
/// Ctrl-A stays the pane's.
#[gpui::test]
fn select_all_highlights_the_focused_pane_for_the_copy_that_follows(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(|window, cx| {
        let mut view = fixture_window(window, cx);
        let mut shown = serde_json::to_value(view.live.snapshot.as_deref().unwrap()).unwrap();
        shown["focused_pane_id"] = "pane".into();
        let snapshot: ClientShellSnapshot = serde_json::from_value(shown).unwrap();
        let mut frame = surface(&["hello there", "second row"], 12);
        frame.boot_id = snapshot.boot_id.clone();
        frame.projection_revision = snapshot.revision;
        view.live.snapshot = Some(Arc::new(snapshot));
        view.live.surface = Some(Arc::new(frame));
        window.focus(&view.focus, cx);
        view
    });
    let clipboard = |cx: &mut gpui::VisualTestContext| {
        cx.update(|_, cx| cx.read_from_clipboard().and_then(|item| item.text()))
    };
    let retained =
        |cx: &mut gpui::VisualTestContext| view.read_with(cx, |view, _| view.selection_retained());
    let available = |cx: &mut gpui::VisualTestContext| {
        cx.update(|window, cx| {
            window.refresh();
            window.draw(cx).clear(cx);
            window.is_action_available(&crate::actions::SelectAll, cx)
        })
    };
    assert!(
        available(cx),
        "Edit > Select All is enabled in the terminal"
    );

    cx.write_to_clipboard(ClipboardItem::new_string("before".into()));
    cx.simulate_keystrokes("cmd-a");
    assert!(retained(cx));
    assert_eq!(clipboard(cx).as_deref(), Some("before"));
    cx.simulate_keystrokes("cmd-c");
    assert_eq!(clipboard(cx).as_deref(), Some("hello there\nsecond row"));

    // Another key drops it, and Ctrl-A is not Select All.
    cx.simulate_keystrokes("x");
    assert!(!retained(cx));
    cx.simulate_keystrokes("ctrl-a");
    assert!(!retained(cx));

    cx.write_to_clipboard(ClipboardItem::new_string("before".into()));
    assert!(available(cx));
    cx.update(|window, cx| window.dispatch_action(Box::new(crate::actions::SelectAll), cx));
    assert!(retained(cx));
    cx.update(|window, cx| window.dispatch_action(Box::new(crate::actions::Copy), cx));
    assert_eq!(clipboard(cx).as_deref(), Some("hello there\nsecond row"));
}

/// Select All needs a terminal to read: with no focused pane there is
/// nothing to take, and Cmd-A selects nothing.
#[gpui::test]
fn select_all_without_a_focused_pane_selects_nothing(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(|window, cx| {
        let mut view = fixture_window(window, cx);
        let mut shown = serde_json::to_value(view.live.snapshot.as_deref().unwrap()).unwrap();
        shown["focused_pane_id"] = serde_json::Value::Null;
        let snapshot: ClientShellSnapshot = serde_json::from_value(shown).unwrap();
        let mut frame = surface(&["hello"], 12);
        frame.boot_id = snapshot.boot_id.clone();
        frame.projection_revision = snapshot.revision;
        view.live.snapshot = Some(Arc::new(snapshot));
        view.live.surface = Some(Arc::new(frame));
        window.focus(&view.focus, cx);
        view
    });
    cx.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    });
    cx.simulate_keystrokes("cmd-a");
    view.read_with(cx, |view, _| assert!(view.selection.is_none()));
}
