use super::*;
use herdr_client::protocol::ToastHerdrPosition;

/// Dismisses the toast showing at `corner` while the next one waits, and
/// returns where it was, where its closing copy is, and where the next is.
fn dismiss_with_another_waiting(
    cx: &mut TestAppContext,
    corner: ToastHerdrPosition,
) -> [gpui::Bounds<gpui::Pixels>; 3] {
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
            notice.position = corner;
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
    [
        first,
        cx.debug_bounds("toast-leaving-local-0").unwrap(),
        cx.debug_bounds("toast-local-1").unwrap(),
    ]
}

#[gpui::test]
fn a_toast_leaving_a_top_corner_stays_put_above_the_next(cx: &mut TestAppContext) {
    let [first, leaving, next] = dismiss_with_another_waiting(cx, ToastHerdrPosition::TopRight);
    // The copy going away stays where the toast was, and the next one comes
    // in beneath it rather than over it.
    assert_eq!(leaving.top(), first.top());
    assert!(next.top() >= leaving.bottom(), "{next:?} below {leaving:?}");
}

#[gpui::test]
fn a_toast_leaving_a_bottom_corner_stays_put_below_the_next(cx: &mut TestAppContext) {
    let [first, leaving, next] = dismiss_with_another_waiting(cx, ToastHerdrPosition::BottomLeft);
    // A bottom stack grows upward: the next comes in above the copy.
    assert_eq!(leaving.bottom(), first.bottom());
    assert!(next.bottom() <= leaving.top(), "{next:?} above {leaving:?}");
}
