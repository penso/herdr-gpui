use super::*;
use crate::update_panel::scroll_thumb;
use gpui::{ScrollDelta, ScrollWheelEvent};

fn viewport() -> Bounds<Pixels> {
    Bounds::new(point(px(10.), px(100.)), size(px(200.), px(100.)))
}

#[core::prelude::v1::test]
fn notes_that_fit_have_no_bar() {
    assert_eq!(scroll_thumb(viewport(), px(0.), px(0.)), None);
    assert_eq!(scroll_thumb(viewport(), px(0.), px(0.5)), None);
}

#[core::prelude::v1::test]
fn the_thumb_spans_the_track_with_the_scroll() {
    let view = viewport();
    let Some(top) = scroll_thumb(view, px(0.), px(300.)) else {
        panic!("overflow shows a bar");
    };
    // A quarter of the content is visible, so the thumb is a quarter tall.
    assert_eq!(top.size.height, px(25.));
    assert_eq!(top.top(), view.top());
    assert!(top.right() <= view.right() && top.left() > view.left());

    let Some(end) = scroll_thumb(view, px(-300.), px(300.)) else {
        panic!("overflow shows a bar");
    };
    assert_eq!(end.bottom(), view.bottom());
    // An elastic overscroll never pushes the thumb out of its track.
    assert_eq!(scroll_thumb(view, px(-900.), px(300.)), Some(end));
    assert_eq!(scroll_thumb(view, px(50.), px(300.)), Some(top));
}

#[core::prelude::v1::test]
fn a_huge_body_keeps_a_grabbable_thumb() {
    let Some(thumb) = scroll_thumb(viewport(), px(0.), px(1_000_000.)) else {
        panic!("overflow shows a bar");
    };
    assert!(thumb.size.height >= px(16.), "{thumb:?}");
}

fn drawn_thumb(
    cx: &mut gpui::VisualTestContext,
    view: &Entity<HerdrWindow>,
) -> Option<Bounds<Pixels>> {
    cx.update(|_, cx| {
        let handle = &view.read(cx).app_update_notes_scroll;
        scroll_thumb(handle.bounds(), handle.offset().y, handle.max_offset().y)
    })
}

#[gpui::test]
fn long_notes_show_a_scrollbar_that_follows_them(cx: &mut gpui::TestAppContext) {
    let (fixture, cx) = cx.add_window_view(|window, cx| {
        crate::bind_keys(cx);
        let view = cx.new(|cx| fixture_window(window, cx));
        cx.observe(&view, |_, _, cx| cx.notify()).detach();
        SidebarFixture(view)
    });
    let view = cx.update(|_, cx| fixture.read(cx).0.clone());
    cx.simulate_resize(size(px(800.), px(600.)));
    let offered = |notes: String| crate::updater::State::Available {
        version: "20261008.1".into(),
        notes,
    };

    draw_update_state(cx, &view, &offered("- One change".into()));
    assert_eq!(drawn_thumb(cx, &view), None, "short notes show no bar");
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.dismiss_menu(window, cx));
        full_draw(window, cx).clear(cx);
    });

    let long: String = (1..=200).map(|line| format!("- Change {line}\n")).collect();
    draw_update_state(cx, &view, &offered(long.clone()));
    let body = cx.debug_bounds("app-update-notes-body").unwrap();
    let Some(top) = drawn_thumb(cx, &view) else {
        panic!("long notes show a bar on their first frame");
    };
    assert!(
        body.contains(&top.origin) && top.right() <= body.right(),
        "{top:?} outside {body:?}"
    );
    assert!(
        top.top() <= body.top() + px(1.),
        "starts at the top: {top:?}"
    );
    // The text keeps clear of the bar.
    let line = cx.debug_bounds("app-update-notes-line-0").unwrap();
    assert!(line.right() <= top.left(), "{line:?} under {top:?}");

    cx.simulate_event(ScrollWheelEvent {
        position: body.center(),
        delta: ScrollDelta::Pixels(point(px(0.), px(-1_000_000.))),
        ..Default::default()
    });
    cx.update(|window, cx| full_draw(window, cx).clear(cx));
    let Some(end) = drawn_thumb(cx, &view) else {
        panic!("the bar stays while scrolled");
    };
    assert!(
        (end.bottom() - body.bottom()).abs() <= px(1.),
        "reaches the bottom: {end:?} in {body:?}"
    );

    // Reopening starts at the top again, not where the last reader left off.
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.dismiss_menu(window, cx));
        full_draw(window, cx).clear(cx);
    });
    draw_update_state(cx, &view, &offered(long));
    let Some(again) = drawn_thumb(cx, &view) else {
        panic!("long notes show a bar");
    };
    assert_eq!(again.top(), top.top());
}
