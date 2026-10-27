use super::*;

fn label(view: &Entity<HerdrWindow>, cx: &mut gpui::VisualTestContext) -> Option<String> {
    view.read_with(cx, |view, cx| {
        view.sidebar_search
            .input
            .read(cx)
            .label()
            .map(str::to_owned)
    })
}

#[gpui::test]
fn the_field_keeps_one_accessible_name_while_its_placeholder_swaps(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(connected);
    cx.simulate_resize(size(px(800.), px(700.)));
    cx.run_until_parked();
    cx.update(|window, cx| full_draw(window, cx).clear(cx));
    // At rest the placeholder is the heading, which alone names no action.
    assert_eq!(label(&view, cx).as_deref(), Some("Search spaces"));

    type_query(&view, "sidebar-child", cx);
    assert_eq!(label(&view, cx).as_deref(), Some("Search spaces"));
}
