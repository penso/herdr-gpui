use super::*;

/// The keys and `e` move from the row at the top of the view. Once drawn,
/// gpui's own `logical_scroll_top` says row 0 for a uniform list, so this
/// checks the top row against the rows actually drawn.
#[gpui::test]
fn keys_move_from_the_row_at_the_top_of_the_view(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("main.rs");
    let source: String = (1..=200).map(|i| format!("fn f{i}() {{}}\n")).collect();
    std::fs::write(&path, source).unwrap();
    let (view, cx) = window(cx);
    let target = EditorTarget {
        path,
        line: Some(1),
    };
    cx.update(|window, cx| view.update(cx, |view, cx| view.open_code_view(&target, window, cx)));
    cx.run_until_parked();
    let id = view.read_with(cx, |view, _| *view.code_views.keys().next().unwrap());
    let top =
        |cx: &mut VisualTestContext| view.read_with(cx, |view, _| view.code_views[&id].top_row());
    let pending = |cx: &mut VisualTestContext| {
        view.read_with(cx, |view, _| {
            let scroll = view.code_views[&id].scroll.0.borrow();
            scroll.deferred_scroll_to_item.is_some()
        })
    };
    // Drawn, the top row is the first one the list draws.
    let drawn = |cx: &mut VisualTestContext| {
        draw(cx);
        assert!(!pending(cx));
        let row = top(cx);
        // `debug_bounds` takes a `&'static str`.
        let line = |row: usize| format!("code-line-{row}").leak();
        assert!(cx.debug_bounds(line(row)).is_some(), "{row}");
        if let Some(above) = row.checked_sub(1) {
            assert!(cx.debug_bounds(line(above)).is_none(), "{row}");
        }
        row
    };
    drawn(cx);

    // A jump is the top row until it is drawn; drawn, its line is centred,
    // so the top is above it.
    view.update(cx, |view, _| {
        view.code_views.get_mut(&id).unwrap().go_to(150)
    });
    assert!(pending(cx));
    assert_eq!(top(cx), 149);
    let centred = drawn(cx);
    assert!((100..149).contains(&centred), "{centred}");

    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            let focus = view.code_views[&id].focus.clone();
            window.focus(&focus, cx);
        })
    });
    cx.simulate_keystrokes("j");
    assert_eq!(drawn(cx), centred + 1);
    cx.simulate_keystrokes("space");
    assert_eq!(drawn(cx), centred + 21);
    cx.simulate_keystrokes("k");
    assert_eq!(drawn(cx), centred + 20);
    cx.simulate_keystrokes("home");
    assert_eq!(drawn(cx), 0);
}
