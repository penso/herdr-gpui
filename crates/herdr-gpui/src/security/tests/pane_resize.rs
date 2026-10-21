//! An open alert sits in a pane beside the list, as an issue's page does:
//! the list keeps the room, and the pane's left edge resizes it or drags it
//! shut.
use super::pane::shown;
use super::section::draw;
use crate::orchestrator::pane::PANE_DEFAULT;
use gpui::{Modifiers, MouseButton, Pixels, Point, VisualTestContext, point, px};

#[gpui::test]
fn an_open_alert_sits_in_a_pane_its_edge_resizes(cx: &mut gpui::TestAppContext) {
    let (view, cx) = shown(cx);
    let row = cx.debug_bounds("security-alert").unwrap();
    cx.simulate_click(row.center(), Modifiers::default());
    draw(cx);
    let pane = cx.debug_bounds("security-pane").unwrap();
    let list = cx.debug_bounds("security-list").unwrap();
    assert_eq!(pane.size.width, px(PANE_DEFAULT));
    // The list keeps every pixel the pane leaves.
    let section = cx.debug_bounds("security-section").unwrap();
    assert_eq!(list.left(), section.left());
    assert_eq!(list.right(), pane.left());
    assert_eq!(pane.right(), section.right());

    // Dragging the edge 100 pixels left widens the pane by as much.
    let handle = cx.debug_bounds("security-pane-handle").unwrap();
    let start = handle.center();
    let to = point(start.x - px(100.), start.y);
    drag(cx, start, to);
    draw(cx);
    let wider = cx.debug_bounds("security-pane").unwrap();
    assert!(
        (wider.size.width - px(PANE_DEFAULT + 100.)).abs() <= px(4.),
        "{wider:?}"
    );

    // Dragged nearly to the right edge, the page closes.
    let handle = cx.debug_bounds("security-pane-handle").unwrap();
    let start = handle.center();
    let to = point(wider.right() - px(20.), start.y);
    drag(cx, start, to);
    draw(cx);
    assert!(cx.debug_bounds("security-pane").is_none());
    assert!(view.read_with(cx, |view, _| view.security.opened().is_none()));
}

/// Presses at `from`, moves a little to start the drag, then to `to`.
fn drag(cx: &mut VisualTestContext, from: Point<Pixels>, to: Point<Pixels>) {
    cx.simulate_mouse_down(from, MouseButton::Left, Modifiers::default());
    let nudge = if to.x < from.x { px(-10.) } else { px(10.) };
    cx.simulate_mouse_move(
        point(from.x + nudge, from.y),
        MouseButton::Left,
        Modifiers::default(),
    );
    cx.simulate_mouse_move(to, MouseButton::Left, Modifiers::default());
    cx.simulate_mouse_up(to, MouseButton::Left, Modifiers::default());
}
