use super::*;
use herdr_client::protocol::ToastHerdrPosition;

#[gpui::test]
fn a_dismissed_toast_slides_out_where_it_was_above_the_next(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(fixture_window);
    cx.simulate_resize(size(px(1000.), px(600.)));
    let draw = |cx: &mut gpui::VisualTestContext| {
        cx.update(|window, cx| {
            window.refresh();
            window.draw(cx).clear(cx);
        });
    };
    view.update(cx, |view, _| {
        view.toast_motion.live = true;
        let notices = ["first", "second"].map(|title| {
            let mut notice = Notice::new(notification(title), Instant::now()).preview();
            notice.position = ToastHerdrPosition::TopRight;
            notice
        });
        view.endpoints[0].toasts.receive(notices);
    });
    draw(cx);
    let first = cx.debug_bounds("toast-local-0").unwrap();
    assert!(cx.debug_bounds("toast-local-1").is_none());
    view.update(cx, |view, _| view.endpoints[0].toasts.dismiss(0));
    draw(cx);
    view.update(cx, |view, _| {
        view.toast_motion
            .hold(crate::motion::now() + std::time::Duration::from_secs(3600));
    });
    draw(cx);
    // The copy going away stays where the toast was, and the next one comes
    // in beneath it rather than under it.
    let leaving = cx.debug_bounds("toast-leaving-local-0").unwrap();
    let next = cx.debug_bounds("toast-local-1").unwrap();
    assert_eq!(leaving.top(), first.top());
    assert!(next.top() >= leaving.bottom(), "{next:?} below {leaving:?}");
}
