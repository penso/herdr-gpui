//! Pages draw above the status bar's tooltips, so while the pointer is over
//! the bar, the pages in the band above it step aside, as for a menu.
use super::*;
use crate::menu::Cover;
use gpui::{Bounds, Modifiers, point, px, size};

#[gpui::test]
fn hovering_the_status_bar_covers_the_band_above_it(cx: &mut gpui::TestAppContext) {
    let (view, cx) = window(cx);
    draw(cx);
    let bar = cx.debug_bounds("connection-status").unwrap();
    cx.simulate_mouse_move(bar.center(), None, Modifiers::default());
    draw(cx);
    let band = view
        .read_with(cx, |view, _| view.browser.tooltip_band)
        .unwrap();
    assert_eq!(band.bottom(), bar.top());
    assert!(band.size.height >= px(200.));
    // A page reaching the bar is covered; one well above it is not.
    let page = |top: f32, bottom: f32| {
        Some(Bounds::new(
            point(px(400.), px(top)),
            size(px(300.), px(bottom - top)),
        ))
    };
    let cover = Cover::Panel(band);
    assert!(cover.covers(page(40., f32::from(bar.top()))));
    assert!(!cover.covers(page(0., f32::from(band.top()) - 10.)));

    cx.simulate_mouse_move(point(bar.center().x, px(100.)), None, Modifiers::default());
    draw(cx);
    view.read_with(cx, |view, _| {
        assert!(view.browser.tooltip_band.is_none());
        assert!(!view.pages_covered());
    });
}
