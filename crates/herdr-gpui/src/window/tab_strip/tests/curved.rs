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

/// A curved strip at the bottom of a fresh window, drawn once.
fn bottom_strip(cx: &mut TestAppContext) -> (Entity<HerdrWindow>, &mut VisualTestContext) {
    let shared =
        crate::herdr_settings::Settings::parse_text("[ui]\ntab_bar_position = 'bottom'").unwrap();
    let (view, cx) = cx.add_window_view(move |window, cx| {
        let mut view = crate::sidebar::layout_tests::fixture_window(window, cx);
        view.config.tab_layout = TabLayoutMode::Curved;
        let snapshot: herdr_client::protocol::ClientShellSnapshot = serde_json::from_str(
            include_str!("../../../../../herdr-protocol/tests/fixtures/endpoint-snapshot-v1.json"),
        )
        .unwrap();
        view.live.snapshot = Some(Arc::new(snapshot));
        view.settings.shared = Some(shared);
        view
    });
    cx.simulate_resize(size(px(1200.), px(600.)));
    redraw(cx);
    (view, cx)
}

/// The card paints before a strip below it, so it must read where the body
/// lies this frame, placed while prepainting, not where it lay the last.
#[gpui::test]
fn a_card_over_a_bottom_strip_shapes_its_corners_on_the_first_draw(cx: &mut TestAppContext) {
    let (view, cx) = bottom_strip(cx);
    let card = cx.debug_bounds("content-card").unwrap();
    let tab = cx.debug_bounds("tab-w1:t1").unwrap();
    assert!(tab.top() >= card.bottom() - px(0.5), "{tab:?} {card:?}");
    let (corners, (left, right)) = view.read_with(cx, |view, _| {
        let group = view.active_group().unwrap();
        let cells = view.browser.tab_body.cells(group).unwrap();
        (cells.corners.get().unwrap(), cells.visible.get().unwrap())
    });
    // The card's own bounds are inset from the wrapper's.
    let (edge_left, edge_right) = (card.left() + px(6.), card.right() - px(6.));
    let expect = |clearance: Pixels| {
        let k = (f32::from(clearance) / 10.).clamp(0., 1.);
        10. * k * k * (3. - 2. * k)
    };
    assert_eq!(corners[0], 10.);
    assert_eq!(corners[1], 10.);
    assert!(
        (corners[2] - expect(left - edge_left)).abs() < 0.01,
        "{corners:?}"
    );
    assert!(
        (corners[3] - expect(edge_right - right)).abs() < 0.01,
        "{corners:?}"
    );
    // The chosen first tab's shoulder meets the card's edge, so the corner
    // beside it is square on the very first draw.
    assert!(corners[2] < 10., "{corners:?} {left:?} {card:?}");
}

/// The body lies over the tab list rather than in it, so tabs that fit
/// never scroll, and no thumb shows along the strip's foot.
#[gpui::test]
fn tabs_that_fit_do_not_scroll(cx: &mut TestAppContext) {
    let (view, cx) = shared_tabs(cx, &["a", "b"], "tab_bar_position = 'top'");
    view.update(cx, |view, cx| {
        view.config.tab_layout = TabLayoutMode::Curved;
        cx.notify();
    });
    redraw(cx);
    redraw(cx);
    assert!(cx.debug_bounds("tab-body").is_some());
    view.update(cx, |view, _| {
        let group = view.active_group().unwrap();
        assert_eq!(view.strip_scroll(group).max_offset().x, px(0.));
        assert_eq!(view.strip_thumb(group), None);
    });
}
