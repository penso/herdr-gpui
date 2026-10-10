use super::*;

#[gpui::test]
fn the_badge_leads_the_card_and_an_emoji_leaves_the_title(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(fixture_window);
    cx.simulate_resize(size(px(1000.), px(600.)));
    cx.update(|window, cx| {
        view.update(cx, |view, _| {
            view.endpoints[0].toasts.receive([Notice::new(
                notification("🚀 Deployed"),
                Instant::now(),
            )
            .preview()]);
        });
        window.draw(cx).clear(cx);
    });
    let card = cx.debug_bounds("toast-local-0").unwrap();
    let badge = cx.debug_bounds("toast-badge-local-0").unwrap();
    let dismiss = cx.debug_bounds("toast-dismiss-local-0").unwrap();
    assert_eq!(badge.size, size(px(28.), px(28.)));
    assert_eq!(badge.left(), card.left() + px(11.));
    assert!(badge.top() >= card.top() && badge.bottom() <= card.bottom());
    assert!(badge.right() < dismiss.left());
    view.read_with(cx, |view, _| {
        // The emoji only moves on screen; the notice and OS banner keep it.
        assert_eq!(view.endpoints[0].toasts.entries[0].1.title, "🚀 Deployed");
    });
}
