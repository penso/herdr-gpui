use super::*;

#[gpui::test]
fn turning_the_search_off_hides_the_field_and_its_results(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(connected);
    cx.simulate_resize(size(px(800.), px(700.)));
    type_query(&view, "sidebar-child", cx);
    assert!(cx.debug_bounds("sidebar-search-results").is_some());

    view.update(cx, |view, cx| {
        view.config.show_sidebar_search = false;
        cx.notify();
    });
    cx.update(|window, cx| full_draw(window, cx).clear(cx));
    // The list comes back whole, starting right under the heading.
    assert!(cx.debug_bounds("sidebar-search").is_none());
    assert!(cx.debug_bounds("sidebar-search-results").is_none());
    let heading = cx.debug_bounds("header-spaces").unwrap();
    let list = cx.debug_bounds("spaces-scroll").unwrap();
    assert_eq!(list.top(), heading.bottom());

    view.update(cx, |view, cx| {
        view.config.show_sidebar_search = true;
        cx.notify();
    });
    cx.update(|window, cx| full_draw(window, cx).clear(cx));
    assert!(cx.debug_bounds("sidebar-search").is_some());
}
