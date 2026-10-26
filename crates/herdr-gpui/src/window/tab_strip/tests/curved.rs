use super::*;
use crate::{browser::tab_body::Span, config::TabLayoutMode};

fn redraw(cx: &mut VisualTestContext) {
    cx.update(|window, cx| crate::sidebar::layout_tests::full_draw(window, cx).clear(cx));
}

/// The body's span and visible run, as the strip's paint shared them.
fn body(view: &Entity<HerdrWindow>, cx: &mut VisualTestContext) -> (Span, (Pixels, Pixels)) {
    view.read_with(cx, |view, _| {
        let group = view.active_group().unwrap();
        let cells = view.browser.tab_body.cells(group).unwrap();
        (cells.drawn.get().unwrap(), cells.visible.get().unwrap())
    })
}

#[gpui::test]
fn the_body_flares_past_the_chosen_tab_into_an_inset_card(cx: &mut TestAppContext) {
    let (view, cx) = layouts::strip(cx, TabLayoutMode::Curved);
    let tab = cx.debug_bounds("tab-w1:t1").unwrap();
    let list = cx.debug_bounds("tab-body").unwrap();
    let (span, (left, right)) = body(&view, cx);
    assert!((list.left() + px(span.left) - tab.left()).abs() <= px(0.5));
    assert!((px(span.width) - tab.size.width).abs() <= px(0.5));
    // Shoulders flare ten pixels past each side of the tab.
    assert!(
        (tab.left() - left - px(10.)).abs() <= px(0.5),
        "{left:?} {tab:?}"
    );
    assert!(
        (right - tab.right() - px(10.)).abs() <= px(0.5),
        "{right:?} {tab:?}"
    );
    // The card sits under the strip, its content inset from the chrome.
    let card = cx.debug_bounds("content-card").unwrap();
    assert!(
        (card.top() - tab.bottom()).abs() <= px(0.5),
        "{card:?} {tab:?}"
    );
    // The strip leaves room for the first body's shoulder inside the card.
    assert!(left >= card.left() + px(6.) - px(0.5), "{left:?} {card:?}");
}

#[gpui::test]
fn choosing_a_tab_slides_the_body_from_where_it_drew(cx: &mut TestAppContext) {
    let (view, cx) = layouts::strip(cx, TabLayoutMode::Curved);
    let (before, _) = body(&view, cx);
    view.update(cx, |view, cx| {
        let snapshot = Arc::make_mut(view.live.snapshot.as_mut().unwrap());
        for tab in &mut snapshot.tabs {
            tab.focused = tab.tab_id == "zoomy";
        }
        cx.notify();
    });
    redraw(cx);
    // The first frame of the slide draws where the body was, under a tab it
    // is about to leave; the chosen tab is elsewhere.
    let (first, _) = body(&view, cx);
    assert_eq!(first, before);
    let zoomy = cx.debug_bounds("tab-zoomy").unwrap();
    let list = cx.debug_bounds("tab-body").unwrap();
    assert!((list.left() + px(before.left) - zoomy.left()).abs() > px(1.));
}

#[gpui::test]
fn other_layouts_keep_their_own_frame(cx: &mut TestAppContext) {
    for &mode in TabLayoutMode::ALL {
        if mode == TabLayoutMode::Curved {
            continue;
        }
        let (_view, cx) = layouts::strip(cx, mode);
        assert!(cx.debug_bounds("tab-body").is_none(), "{mode}");
        assert!(cx.debug_bounds("content-card").is_none(), "{mode}");
    }
}
