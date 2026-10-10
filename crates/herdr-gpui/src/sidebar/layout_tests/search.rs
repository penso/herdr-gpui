use super::*;

mod hidden;
mod scrolling;

fn connected(window: &mut Window, cx: &mut Context<HerdrWindow>) -> HerdrWindow {
    crate::bind_keys(cx);
    let mut view = fixture_window(window, cx);
    view.live.snapshot = Some(Arc::new(snapshot(6)));
    view.live.status = crate::state::ConnectionStatus::Connected;
    view.live.supports_surface = true;
    view.endpoints[0].live = view.live.clone();
    view
}

fn type_query(view: &Entity<HerdrWindow>, text: &str, cx: &mut gpui::VisualTestContext) {
    cx.update(|window, cx| {
        let focus = view.read(cx).sidebar_search.input.read(cx).focus.clone();
        window.focus(&focus, cx);
        full_draw(window, cx).clear(cx);
    });
    cx.simulate_input(text);
    cx.run_until_parked();
    cx.update(|window, cx| full_draw(window, cx).clear(cx));
}

#[gpui::test]
fn a_query_swaps_the_spaces_list_for_results_and_enter_opens_one(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(connected);
    cx.simulate_resize(size(px(800.), px(700.)));
    cx.run_until_parked();
    cx.update(|window, cx| full_draw(window, cx).clear(cx));
    let field = cx.debug_bounds("sidebar-search").unwrap();
    let list = cx.debug_bounds("spaces-scroll").unwrap();
    assert!(
        field.bottom() <= list.top(),
        "the field sits above the list"
    );
    assert!(cx.debug_bounds("sidebar-search-results").is_none());

    type_query(&view, "sidebar-child", cx);
    assert!(cx.debug_bounds("spaces-scroll").is_none());
    assert!(cx.debug_bounds("sidebar-search-results").is_some());
    assert!(cx.debug_bounds("sidebar-search-clear").is_some());
    // Two worktrees and their two branches.
    assert!(cx.debug_bounds("sidebar-search-hit-3").is_some());
    assert!(cx.debug_bounds("sidebar-search-hit-4").is_none());

    view.read_with(cx, |view, _| assert!(view.pending_navigation.is_none()));
    cx.simulate_keystrokes("up down down up enter");
    cx.run_until_parked();
    cx.update(|window, cx| {
        let view = view.read(cx);
        assert_eq!(
            view.pending_navigation,
            Some(crate::NavigationTarget::Workspace("w5".into()))
        );
        // Opening a result clears the search and hands the keyboard back.
        assert!(view.sidebar_search.query().is_none());
        assert!(view.focus.is_focused(window));
        full_draw(window, cx).clear(cx);
    });
    assert!(cx.debug_bounds("spaces-scroll").is_some());
}

#[gpui::test]
fn escape_clears_a_query_without_matches(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(connected);
    cx.simulate_resize(size(px(800.), px(700.)));
    type_query(&view, "no such thing", cx);
    assert!(cx.debug_bounds("sidebar-search-hit-0").is_none());
    assert!(cx.debug_bounds("sidebar-search-results").is_some());
    cx.simulate_keystrokes("enter escape");
    cx.run_until_parked();
    cx.update(|window, cx| {
        let view = view.read(cx);
        assert!(view.pending_navigation.is_none());
        assert!(view.sidebar_search.query().is_none());
        assert!(view.focus.is_focused(window));
        full_draw(window, cx).clear(cx);
    });
    assert!(cx.debug_bounds("spaces-scroll").is_some());
}
