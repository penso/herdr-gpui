//! Native pages draw above everything the window paints, so a page that a
//! toast or the file-transfer card reaches steps aside while it shows, as
//! for a menu; a page it does not reach keeps showing.
use super::*;
use crate::{browser::view::present::covered, menu::Cover};
use gpui::{Bounds, point, px, size};
use herdr_client::protocol::SemanticNotificationKind;

fn rect(x: f32, y: f32, width: f32, height: f32) -> Bounds<gpui::Pixels> {
    Bounds::new(point(px(x), px(y)), size(px(width), px(height)))
}

#[test]
fn a_page_steps_aside_only_for_the_overlays_it_reaches() {
    // A toast at the window's top right, over the right-hand group.
    let toast = [rect(1048., 52., 340., 90.)];
    // VS Code filling the right-hand group, or the whole editor area.
    let right = Some(rect(700., 40., 700., 760.));
    let whole = Some(rect(240., 40., 1160., 760.));
    // A page in the left group, clear of the toast.
    let left = Some(rect(240., 40., 450., 760.));
    assert!(covered(None, &toast, right));
    assert!(covered(None, &toast, whole));
    assert!(!covered(None, &toast, left));
    // Nothing over the pages, or a page not yet laid out, covers nothing.
    assert!(!covered(None, &[], whole));
    assert!(!covered(None, &toast, None));
    // A menu's cover still counts on its own.
    assert!(covered(Some(Cover::All), &[], left));
}

#[gpui::test]
fn a_toast_covers_the_pages_it_reaches_while_it_shows(cx: &mut gpui::TestAppContext) {
    let (view, cx) = window(cx);
    cx.simulate_resize(size(px(1400.), px(800.)));
    draw(cx);
    assert!(view.read_with(cx, |view, _| view.presented_overlays().is_empty()));

    view.update(cx, |view, cx| {
        view.config.notifications.delay_seconds = 3600;
        view.show_toast_preview(SemanticNotificationKind::Finished, cx);
    });
    // The toast draws after the pages are presented: the frame it first
    // draws in asks for another, which presents them around it.
    draw(cx);
    draw(cx);
    let presented = view.read_with(cx, |view, _| view.presented_overlays());
    assert_eq!(presented.len(), 1, "{presented:?}");
    let toast = presented[0];
    assert!(toast.size.width > px(0.) && toast.size.height > px(0.));
    // Its stack sits at the window's right, where VS Code in the right-hand
    // group, or filling the editor area, would hide it.
    assert!(toast.right() > px(1300.));
    assert!(view.read_with(cx, |view, _| view.pages_covered()));

    // Once it is gone, pages show again in the very next frame.
    view.update(cx, |view, _| {
        for endpoint in &mut view.endpoints {
            endpoint.toasts.entries.clear();
        }
    });
    draw(cx);
    view.read_with(cx, |view, _| {
        assert!(view.presented_overlays().is_empty());
        assert!(!view.pages_covered());
    });
}
