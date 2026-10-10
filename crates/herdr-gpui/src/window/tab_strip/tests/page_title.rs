//! A page's title comes in after its tab opened, often once the tab is in
//! view and done growing. The tab then widens, so a crowded strip brings it
//! into view again: otherwise its end, with its close button, is left past
//! the strip's edge until another tab is chosen.
use super::*;
use crate::browser::{Store, TabId};

const TITLE: &str = "feat: scroll pane history smoothly, following the trackpad";

fn frames(view: &Entity<HerdrWindow>, cx: &mut VisualTestContext) {
    let draw = |cx: &mut VisualTestContext| {
        cx.update(|window, cx| crate::sidebar::layout_tests::full_draw(window, cx).clear(cx))
    };
    // The strip sees a new tab as it draws it; then it is done growing, as
    // it is a moment later.
    draw(cx);
    view.update(cx, |view, _| view.settle_tabs());
    for _ in 0..6 {
        draw(cx);
    }
}

fn close_button(id: TabId) -> &'static str {
    Box::leak(format!("close-browser-tab-{id}").into_boxed_str())
}

#[gpui::test]
fn a_chosen_page_stays_in_view_when_its_title_comes_in(cx: &mut TestAppContext) {
    let (view, cx) = crowded(cx, false);
    // A page opens at the strip's end with no title yet, as one an agent
    // opens does, and is shown.
    let opened = cx.update(|_, cx| {
        let scope = crate::browser::scope(&view.read(cx).endpoints[0]);
        Store::update(cx, |store| store.open(scope, "w1", None, None)).unwrap()
    });
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.show_browser_tab_in(None, opened, window, cx)
        })
    });
    frames(&view, cx);
    let (left, right) = viewport(cx);
    let close = cx.debug_bounds(close_button(opened)).unwrap();
    assert!(
        close.left() >= left && close.right() <= right + px(0.5),
        "{close:?} ({left:?}, {right:?})"
    );

    // Its title comes in once it is settled, and it widens.
    cx.update(|_, cx| Store::update(cx, |store| store.visited(opened, None, Some(TITLE))));
    frames(&view, cx);
    let (left, right) = viewport(cx);
    let close = cx.debug_bounds(close_button(opened)).unwrap();
    assert!(
        close.left() >= left && close.right() <= right + px(0.5),
        "the close button is past the strip's edge: {close:?} ({left:?}, {right:?})"
    );
}
