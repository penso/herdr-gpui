//! The pill on the sidebar's width edge, whose drag the sidebar tracks
//! itself and whose release its own listener stops.
use super::*;
use gpui::MouseButton;

fn pill(cx: &mut gpui::VisualTestContext) -> Option<Pixels> {
    cx.debug_bounds("sidebar-resize-pill")
        .map(|pill| pill.size.height)
}

fn draw(cx: &mut gpui::VisualTestContext) {
    cx.update(|window, cx| full_draw(window, cx).clear(cx));
}

#[gpui::test]
fn the_sidebar_edge_s_pill_follows_a_drag_and_hides_after_release(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(fixture_window);
    cx.simulate_resize(size(px(800.), px(600.)));
    draw(cx);
    let edge = cx.debug_bounds("sidebar-resize").unwrap();
    assert_eq!(pill(cx), None, "nothing at rest");
    cx.simulate_mouse_move(edge.center(), None, Modifiers::default());
    draw(cx);
    assert_eq!(pill(cx), Some(px(20.)), "hovered");
    // The sidebar's drag starts with the press, and it says so.
    cx.simulate_mouse_down(edge.center(), MouseButton::Left, Modifiers::default());
    draw(cx);
    assert!(view.read_with(cx, |view, _| view.sidebar_drag.is_some()));
    assert_eq!(pill(cx), Some(px(44.)), "dragging");
    // The edge follows the pointer, so the pill rides along with it.
    let to = edge.center() + point(px(60.), px(0.));
    cx.simulate_mouse_move(to, MouseButton::Left, Modifiers::default());
    draw(cx);
    let moved = cx.debug_bounds("sidebar-resize").unwrap();
    assert!((moved.center().x - to.x).abs() <= px(1.), "{moved:?}");
    assert_eq!(pill(cx), Some(px(44.)), "still dragging");
    // The sidebar's own release listener stops the event; the grip still
    // hears it, so the pill is not left held.
    cx.simulate_mouse_up(to, MouseButton::Left, Modifiers::default());
    draw(cx);
    assert!(view.read_with(cx, |view, _| view.sidebar_drag.is_none()));
    assert_eq!(pill(cx), Some(px(20.)), "released on the edge");
    cx.simulate_mouse_move(to + point(px(200.), px(0.)), None, Modifiers::default());
    draw(cx);
    assert_eq!(pill(cx), None, "the pointer left");
}
