use super::*;
use crate::config::{Padding, Sides};
use gpui::px;

/// The find bar belongs to the pane it searches, so it keeps its place in
/// the top right corner of the cells, not of the padding around them.
#[gpui::test]
fn find_bar_sits_over_the_cells_inside_the_terminal_padding(cx: &mut TestAppContext) {
    let (view, _peer, cx) = open(cx, &["pane.copy_search"]);
    view.update(cx, |view, cx| {
        view.config.terminal_padding = Padding {
            x: Sides::new(16., 8.).unwrap(),
            y: Sides::new(4., 12.).unwrap(),
        };
        cx.notify();
    });
    find(&view, cx);

    let bar = cx.debug_bounds("find-bar").unwrap();
    let terminal = cx.debug_bounds("terminal").unwrap();
    let (right, top) = view.read_with(cx, |view, _| {
        let rect = view.live.surface.as_ref().unwrap().panes[0].rect;
        assert_eq!(view.bounds.left(), terminal.left() + px(16.));
        assert_eq!(view.bounds.top(), terminal.top() + px(4.));
        (
            view.bounds.left() + px(f32::from(rect.x + rect.width) * view.cell_width),
            view.bounds.top() + px(f32::from(rect.y) * view.config.terminal.line_height()),
        )
    });
    assert!((bar.right() - (right - px(6.))).abs() < px(0.5));
    assert!((bar.top() - (top + px(6.))).abs() < px(0.5));
}
