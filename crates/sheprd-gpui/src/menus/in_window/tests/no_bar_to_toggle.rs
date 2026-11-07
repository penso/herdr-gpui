//! Where the system draws the menus the command has nothing to hide, and the
//! window's chrome must keep treating the bar as absent either way.

use super::*;
use crate::controls::Command;

#[gpui::test]
fn the_toggle_flips_only_where_the_window_draws_the_bar(cx: &mut TestAppContext) {
    let (view, cx) = window(cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.command(Command::ToggleMenuBar, window, cx)
        })
    });
    let visible = view.read_with(cx, |view, _| view.menu_bar_visible);
    assert_eq!(visible, !crate::menus::IN_WINDOW);
    assert!(!view.read_with(cx, |view, _| view.menu_bar_shown()));
}
