#![allow(clippy::unwrap_used)]

use super::*;
use gpui::{Entity, Modifiers, TestAppContext, TouchPhase, VisualTestContext};

mod resize;
mod scrolling;

fn field(cx: &mut TestAppContext) -> (Entity<MultilineInput>, &mut VisualTestContext) {
    let (input, cx) = cx.add_window_view(|_, cx| MultilineInput::new(cx));
    cx.simulate_resize(size(px(700.), px(500.)));
    cx.update(|window, cx| {
        let focus = input.read(cx).focus.clone();
        window.focus(&focus, cx);
    });
    draw(cx);
    draw(cx);
    (input, cx)
}

fn draw(cx: &mut VisualTestContext) {
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.run_until_parked();
}

fn scroll_up(input: &Entity<MultilineInput>, cx: &mut VisualTestContext) {
    let position = input.read_with(cx, |input, _| input.bounds.unwrap().center());
    cx.simulate_event(ScrollWheelEvent {
        position,
        delta: ScrollDelta::Lines(point(0., 12.)),
        modifiers: Modifiers::default(),
        touch_phase: TouchPhase::Moved,
    });
    draw(cx);
}

fn assert_caret_visible(input: &Entity<MultilineInput>, cx: &VisualTestContext) {
    input.read_with(cx, |input, _| {
        let height = px(input.font.line_height());
        let caret =
            caret_position(input.lines.as_ref().unwrap(), height, input.edit.cursor).unwrap();
        let top = caret.y - input.scroll;
        assert!(top >= px(0.), "caret above viewport: {top:?}");
        assert!(
            top + height <= input.bounds.unwrap().size.height + px(0.5),
            "caret below viewport: {top:?}"
        );
    });
}
