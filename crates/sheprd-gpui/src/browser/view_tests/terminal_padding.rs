use super::*;
use gpui::px;

/// Terminal padding pads a terminal, whose first column would otherwise
/// touch the sidebar. A page's toolbar pads its own content, so it sits
/// against the sidebar as its group's tab strip does.
#[gpui::test]
fn terminal_padding_does_not_move_a_page(cx: &mut gpui::TestAppContext) {
    let (view, cx) = window(cx);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.config.terminal_padding.x = crate::config::Sides::new(16., 16.).unwrap();
            view.sidebar_visible = true;
            cx.notify();
        });
    });
    draw(cx);
    let sidebar = cx.debug_bounds("sidebar").unwrap();
    // The padding is in effect: the terminal starts past it.
    assert_eq!(
        view.read_with(cx, |view, _| view.bounds.origin.x),
        sidebar.right() + px(16.),
    );

    cx.update(|window, cx| view.update(cx, |view, cx| view.open_browser_tab(None, window, cx)));
    draw(cx);
    let group = cx.debug_bounds("group").unwrap();
    let toolbar = cx.debug_bounds("browser-toolbar").unwrap();
    assert_eq!(cx.debug_bounds("sidebar").unwrap(), sidebar);
    assert_eq!(group.left(), sidebar.right());
    // The toolbar's own color reaches the sidebar, with no strip between.
    assert_eq!(toolbar.left(), sidebar.right());
    assert_eq!(toolbar.right(), group.right());
}
