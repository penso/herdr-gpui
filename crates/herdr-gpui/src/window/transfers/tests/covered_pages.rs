//! The file-transfer card shows over the groups, so the native pages it
//! reaches step aside while it shows, keeping its progress and its Cancel
//! visible and clickable.
use super::*;

#[gpui::test]
fn the_transfer_card_covers_the_pages_it_reaches_while_it_shows(cx: &mut TestAppContext) {
    let (fixture, cx) = cx.add_window_view(fixture);
    let view = fixture.read_with(cx, |fixture, _| fixture.view.clone().unwrap());
    let peer = Peer::new();
    // The fixture draws the card alone, so each frame presents the pages
    // first, as the window's render does before it draws the card.
    let draw = |cx: &mut VisualTestContext| {
        view.update(cx, |view, cx| {
            view.present_browser(cx);
            cx.notify();
        });
        cx.update(|window, cx| {
            window.refresh();
            window.draw(cx).clear(cx);
        })
    };
    view.update(cx, |view, cx| {
        peer.prepare(view, cx);
        view.file_transfer = Some(pending(view, InputTarget::Pane("w1:p1".into())));
        view.poll_file_transfer(cx);
    });
    draw(cx);
    draw(cx);
    let card = cx.debug_bounds("file-transfer").unwrap();
    // Measured inside the card's border: what reaches the card reaches it.
    let presented = view.read_with(cx, |view, _| view.presented_overlays());
    assert_eq!(presented.len(), 1, "{presented:?}");
    assert!(
        card.contains(&presented[0].origin),
        "{presented:?} {card:?}"
    );
    assert!(card.size.width - presented[0].size.width <= px(2.));
    assert!(card.size.height - presented[0].size.height <= px(2.));

    view.update(cx, |view, _| view.file_transfer = None);
    draw(cx);
    assert!(view.read_with(cx, |view, _| view.presented_overlays().is_empty()));
}
