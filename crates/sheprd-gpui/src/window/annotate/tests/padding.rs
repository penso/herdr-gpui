use super::*;
use crate::config::{Padding, Sides};

/// The note answers a selection, so it keeps its place under the selected
/// cells and inside the pane, not shifted by the padding around them.
#[gpui::test]
fn note_sits_under_the_selection_inside_the_terminal_padding(cx: &mut TestAppContext) {
    let (view, cx) = window(cx, shown(Some("working"), false));
    view.update(cx, |view, cx| {
        view.config.terminal_padding = Padding {
            x: Sides::new(16., 8.).unwrap(),
            y: Sides::new(4., 12.).unwrap(),
        };
        cx.notify();
    });
    cx.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    });
    select(&view, cx);
    write(&view, cx, "Explain this failure before retrying.");

    let card = cx.debug_bounds("terminal-note").unwrap();
    let terminal = cx.debug_bounds("terminal").unwrap();
    let (cells, cell_width, cell_height) = view.read_with(cx, |view, _| {
        (
            view.bounds,
            view.cell_width,
            view.config.terminal.line_height(),
        )
    });
    assert_eq!(cells.left(), terminal.left() + px(16.));
    assert_eq!(cells.top(), terminal.top() + px(4.));
    // The selection is row 1, so its bottom edge is two rows down the cells.
    let selection_bottom = cells.top() + px(2. * cell_height);
    assert!((card.top() - selection_bottom - px(6.)).abs() < px(1.));
    assert!(card.left() >= cells.left());
    assert!(card.right() <= cells.left() + px(80. * cell_width));
    assert!(card.bottom() <= cells.top() + px(24. * cell_height));
}
