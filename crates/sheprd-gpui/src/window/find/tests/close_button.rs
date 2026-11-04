use super::*;

#[gpui::test]
fn close_button_dismisses_find_and_returns_focus_to_the_pane(cx: &mut TestAppContext) {
    let (view, _peer, cx) = open(cx, &["pane.copy_search"]);
    find(&view, cx);
    let button = cx.debug_bounds("find-close").unwrap();
    let bar = cx.debug_bounds("find-bar").unwrap();
    assert!(button.left() >= bar.left() && button.right() <= bar.right());
    assert!(button.top() >= bar.top() && button.bottom() <= bar.bottom());

    cx.simulate_click(button.center(), Modifiers::default());
    cx.update(|window, cx| {
        let view = view.read(cx);
        assert!(view.find.is_none());
        assert!(view.focus.is_focused(window));
    });
}
