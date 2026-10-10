use super::*;

fn assert_visible(index: usize, cx: &mut gpui::VisualTestContext) {
    let viewport = cx.debug_bounds("sidebar-search-results").unwrap();
    let row = cx
        .debug_bounds(
            [
                "sidebar-search-hit-0",
                "sidebar-search-hit-1",
                "sidebar-search-hit-2",
                "sidebar-search-hit-3",
            ][index],
        )
        .unwrap();
    assert!(
        row.top() >= viewport.top() && row.bottom() <= viewport.bottom(),
        "result {index} {row:?} must be inside {viewport:?}"
    );
}

#[gpui::test]
fn arrow_navigation_reveals_results_across_section_headings(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(connected);
    cx.simulate_resize(size(px(800.), px(400.)));
    type_query(&view, "sidebar-child", cx);
    assert_visible(0, cx);

    for (key, index) in [
        ("down", 1),
        ("down", 2),
        ("down", 3),
        ("down", 3),
        ("up", 2),
        ("up", 1),
        ("up", 0),
        ("up", 0),
    ] {
        cx.simulate_keystrokes(key);
        cx.run_until_parked();
        cx.update(|window, cx| full_draw(window, cx).clear(cx));
        assert_visible(index, cx);
    }
}

#[gpui::test]
fn editing_a_query_reveals_the_reset_selection(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(connected);
    cx.simulate_resize(size(px(800.), px(400.)));
    type_query(&view, "sidebar-child", cx);
    cx.simulate_keystrokes("down down down");
    cx.run_until_parked();
    cx.update(|window, cx| full_draw(window, cx).clear(cx));
    assert_visible(3, cx);

    // A trailing space keeps the same results, so their height cannot clamp
    // the old scroll offset back to the top on its own.
    cx.simulate_input(" ");
    cx.run_until_parked();
    cx.update(|window, cx| full_draw(window, cx).clear(cx));
    assert_visible(0, cx);
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        assert_eq!(
            view.pending_navigation,
            Some(crate::NavigationTarget::Workspace("w4".into()))
        );
    });
}
