use super::*;

#[gpui::test]
fn a_long_task_keeps_the_place_on_its_own_visible_row(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(fixture_window);
    cx.simulate_resize(size(px(1000.), px(600.)));
    cx.update(|window, cx| {
        view.update(cx, |view, _| {
            let mut notice = Notice::new(notification("claude finished"), Instant::now()).preview();
            notice.body = Some(format!("{}\nrepo · tests", "long task ".repeat(12)));
            notice.described = true;
            view.endpoints[0].toasts.receive([notice]);
        });
        window.draw(cx).clear(cx);
    });
    let card = cx.debug_bounds("toast-local-0").unwrap();
    let task = cx.debug_bounds("toast-body-row-local-0-0").unwrap();
    let place = cx.debug_bounds("toast-body-row-local-0-1").unwrap();
    // The task is cut to one row rather than wrapping the place away.
    assert_eq!(task.size.height, place.size.height);
    assert!(place.top() >= task.bottom());
    assert!(place.bottom() <= card.bottom() && place.right() <= card.right());
}
