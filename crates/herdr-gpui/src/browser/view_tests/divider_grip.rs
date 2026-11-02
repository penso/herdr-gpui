//! The pill on a divider between groups, driven by real pointer events
//! across the three pieces the divider is drawn in.
use super::*;
use crate::controls::Command;
use gpui::{Modifiers, MouseButton, Pixels, Point, point, px};

fn pill(cx: &mut VisualTestContext) -> Option<Pixels> {
    cx.debug_bounds("group-divider-0-pill")
        .map(|pill| pill.size.height)
}

fn hover(cx: &mut VisualTestContext, at: Point<Pixels>) {
    cx.simulate_mouse_move(at, None, Modifiers::none());
    draw(cx);
}

fn split(cx: &mut gpui::TestAppContext) -> (Entity<HerdrWindow>, &mut VisualTestContext) {
    let (view, cx) = window(cx);
    draw(cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.command(Command::SplitEditor, window, cx)
        })
    });
    draw(cx);
    (view, cx)
}

#[gpui::test]
fn crossing_a_divider_s_pieces_keeps_one_pill(cx: &mut gpui::TestAppContext) {
    let (_view, cx) = split(cx);
    let line = cx.debug_bounds("group-divider-0").unwrap();
    let left = cx.debug_bounds("group-divider-0-left").unwrap();
    let right = cx.debug_bounds("group-divider-0-right").unwrap();
    assert_eq!(pill(cx), None, "nothing at rest");
    for at in [left.center(), line.center(), right.center(), line.center()] {
        hover(cx, at);
        assert_eq!(pill(cx), Some(px(20.)), "{at:?}");
    }
    // The pill rides on the line, whichever piece is under the pointer.
    let shown = cx.debug_bounds("group-divider-0-pill").unwrap();
    assert!((shown.center().x - line.center().x).abs() <= px(0.5));
    hover(cx, line.center() - point(px(80.), px(0.)));
    assert_eq!(pill(cx), None, "the pointer left every piece");
}

#[gpui::test]
fn dragging_a_divider_grows_the_pill_and_a_release_settles_it(cx: &mut gpui::TestAppContext) {
    let (_view, cx) = split(cx);
    let left = cx.debug_bounds("group-divider-0-left").unwrap();
    hover(cx, left.center());
    cx.simulate_mouse_down(left.center(), MouseButton::Left, Modifiers::none());
    draw(cx);
    assert_eq!(pill(cx), Some(px(28.)), "pressed");
    let to = left.center() - point(px(100.), px(0.));
    cx.simulate_mouse_move(to, Some(MouseButton::Left), Modifiers::none());
    cx.simulate_mouse_move(to, Some(MouseButton::Left), Modifiers::none());
    draw(cx);
    assert_eq!(pill(cx), Some(px(44.)), "dragging");
    cx.simulate_mouse_up(to, MouseButton::Left, Modifiers::none());
    draw(cx);
    // The divider followed the pointer, which is on it still: no longer
    // held, only hovered.
    assert_eq!(pill(cx), Some(px(20.)), "released on the divider");
    hover(cx, to + point(px(200.), px(0.)));
    assert_eq!(pill(cx), None, "the pointer left");
}
