use super::*;
use std::{cell::Cell, rc::Rc};

#[gpui::test]
fn width_changes_request_relayout_and_the_field_grows_and_shrinks_without_editing(
    cx: &mut TestAppContext,
) {
    let (input, cx) = field(cx);
    cx.simulate_input(&"one two three four five ".repeat(8));
    draw(cx);
    draw(cx);
    let before = input.read_with(cx, |input, _| input.bounds.unwrap().size.height);
    let notifications = Rc::new(Cell::new(0));
    let observed = notifications.clone();
    let _subscription =
        cx.update(|_, cx| cx.observe(&input, move |_, _| observed.set(observed.get() + 1)));

    for width in [300., 700.] {
        notifications.set(0);
        cx.simulate_resize(size(px(width), px(500.)));
        draw(cx);
        assert!(
            notifications.get() > 0,
            "a new width must request another frame"
        );
        draw(cx);
        input.read_with(cx, |input, _| {
            let rows = rows_total(input.lines.as_ref().unwrap());
            assert!(
                rows > 1 && rows < MAX_VISIBLE,
                "fixture needs uncapped wrapping: {rows}"
            );
            let bounds = input.bounds.unwrap();
            assert_eq!(bounds.size.width, px(width));
            assert!(
                (bounds.size.height - px(input.font.line_height() * rows as f32)).abs() < px(0.5),
                "height must follow all {rows} wrapped rows: {bounds:?}"
            );
            if width < 700. {
                assert!(bounds.size.height > before);
            } else {
                assert_eq!(bounds.size.height, before);
            }
        });
        assert_caret_visible(&input, cx);
        notifications.set(0);
        draw(cx);
        assert_eq!(
            notifications.get(),
            0,
            "settled widths must not keep repainting"
        );
    }
}

#[gpui::test]
fn reflow_across_the_row_cap_clamps_scroll_and_keeps_the_caret_visible(cx: &mut TestAppContext) {
    let (input, cx) = field(cx);
    cx.simulate_input(&"word ".repeat(80));
    draw(cx);
    draw(cx);
    for width in [120., 700.] {
        cx.simulate_resize(size(px(width), px(500.)));
        draw(cx);
        draw(cx);
        input.read_with(cx, |input, _| {
            let rows = rows_total(input.lines.as_ref().unwrap());
            let height = input.bounds.unwrap().size.height;
            assert!(
                (height - px(input.font.line_height() * rows.min(MAX_VISIBLE) as f32)).abs()
                    < px(0.5)
            );
            if width < 700. {
                assert!(rows > MAX_VISIBLE);
                assert!(input.scroll > px(0.));
            } else {
                assert!(rows < MAX_VISIBLE);
                assert_eq!(input.scroll, px(0.));
            }
        });
        assert_caret_visible(&input, cx);
    }
}
