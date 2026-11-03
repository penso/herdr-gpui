use super::*;
use crate::config::Sides;

/// `[zen] width` counts the terminal's columns, so the column also makes
/// room for `[terminal]`'s padding beside them.
#[gpui::test]
fn a_width_in_columns_leaves_room_for_terminal_padding(cx: &mut TestAppContext) {
    let (view, cx) = window(cx);
    view.update(cx, |view, _| {
        view.config.terminal_padding.x = Sides::new(16., 24.).unwrap();
    });
    set_width(&view, ZenWidth::Columns(80), cx);
    run(&view, Command::ToggleZenMode, cx);
    let cols = view.read_with(cx, |view, _| view.options.surface_size.cols);
    assert_eq!(cols, 80);
}
