//! The main window under `en-XB`: the sidebar, its resize edge, and Back and
//! Forward mirror, while the terminal area keeps its left-to-right layout
//! and the tab row stops standing in for the title bar.
use super::*;
use crate::sidebar::layout_tests::{SidebarFixture, full_draw};
use gpui::{AppContext as _, Entity, Modifiers, MouseButton, VisualTestContext, point, px, size};
use herdr_i18n::Pseudo;

fn fixture(cx: &mut gpui::TestAppContext) -> (Entity<HerdrWindow>, &mut VisualTestContext) {
    crate::i18n::show(&Pseudo::Bidi.locale());
    let (fixture, cx) = cx.add_window_view(|window, cx| {
        let view = cx.new(|cx| fixture_window(window, cx));
        cx.observe(&view, |_, _, cx| cx.notify()).detach();
        SidebarFixture(view)
    });
    cx.simulate_resize(size(px(1000.), px(700.)));
    let view = cx.update(|_, cx| fixture.read(cx).0.clone());
    draw(cx);
    (view, cx)
}

fn draw(cx: &mut VisualTestContext) {
    cx.update(|window, cx| {
        window.refresh();
        full_draw(window, cx).clear(cx);
    });
}

#[gpui::test]
fn the_sidebar_leads_from_the_right(cx: &mut gpui::TestAppContext) {
    let (_, cx) = fixture(cx);
    let body = cx.debug_bounds("window-body").unwrap();
    let sidebar = cx.debug_bounds("sidebar").unwrap();
    assert_eq!(sidebar.right(), body.right());
    assert!(sidebar.left() > body.center().x, "{sidebar:?}");
    // Its resize edge faces the content, on its left, inside the border.
    let edge = cx.debug_bounds("sidebar-resize").unwrap();
    assert!((edge.left() - sidebar.left()).abs() <= px(1.), "{edge:?}");
}

#[gpui::test]
fn dragging_the_sidebar_edge_left_widens_it(cx: &mut gpui::TestAppContext) {
    let (view, cx) = fixture(cx);
    let before = cx.debug_bounds("sidebar").unwrap().size.width;
    let start = cx.debug_bounds("sidebar-resize").unwrap().center();
    cx.simulate_mouse_down(start, MouseButton::Left, Modifiers::default());
    let to = point(start.x - px(40.), start.y);
    cx.simulate_mouse_move(to, MouseButton::Left, Modifiers::default());
    cx.simulate_mouse_up(to, MouseButton::Left, Modifiers::default());
    draw(cx);
    let width = view.read_with(cx, |view, _| view.sidebar_width).unwrap();
    assert!(
        (width - f32::from(before) - 40.).abs() <= 1.,
        "{width} {before:?}"
    );
}

#[gpui::test]
fn back_leads_from_the_right_and_the_tabs_leave_the_title_bar(cx: &mut gpui::TestAppContext) {
    let (view, cx) = fixture(cx);
    let back = cx.debug_bounds("titlebar-back").unwrap();
    let forward = cx.debug_bounds("titlebar-forward").unwrap();
    assert!(back.left() >= forward.right(), "{back:?} {forward:?}");
    // The tab row reads left to right with its panes, so the header stays
    // where English shows the tabs in its place.
    assert!(cx.debug_bounds("titlebar").is_some());
    crate::i18n::show(&herdr_i18n::Locale::FALLBACK);
    view.update(cx, |_, cx| cx.notify());
    draw(cx);
    assert!(cx.debug_bounds("titlebar").is_none());
}
