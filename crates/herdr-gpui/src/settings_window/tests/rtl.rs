//! Settings under `en-XB`: the section list leads from the right and
//! resizes by its left edge, and the page's rows set from the right.
use super::*;
use gpui::{Modifiers, MouseButton, point, px, size};
use herdr_i18n::Pseudo;

#[gpui::test]
fn the_section_list_leads_from_the_right_and_resizes_by_its_left_edge(cx: &mut TestAppContext) {
    crate::i18n::show(&Pseudo::Bidi.locale());
    let main = cx.add_window(crate::sidebar::layout_tests::fixture_window);
    let weak = cx.update(|cx| main.update(cx, |_, _, cx| cx.weak_entity()).unwrap());
    let (view, cx) = cx.add_window_view(|window, cx| {
        let view = SettingsWindow::new(weak, cx);
        window.focus(&view.focus, cx);
        view
    });
    cx.simulate_resize(size(px(1200.), px(800.)));
    let draw = |cx: &mut VisualTestContext| {
        cx.update(|window, cx| crate::sidebar::layout_tests::full_draw(window, cx).clear(cx));
    };
    draw(cx);
    let search = cx.debug_bounds("settings-nav-search").unwrap();
    let body = cx.debug_bounds("settings-body").unwrap();
    assert!(search.left() > body.right() - px(1.), "{search:?} {body:?}");
    // The edge faces the page, on the list's left.
    let edge = cx.debug_bounds("settings-navigation-resize").unwrap();
    assert!((edge.left() - px(1200. - 184.)).abs() <= px(1.), "{edge:?}");
    let start = edge.center();
    cx.simulate_mouse_down(start, MouseButton::Left, Modifiers::default());
    cx.simulate_mouse_move(
        point(start.x - px(40.), start.y),
        MouseButton::Left,
        Modifiers::default(),
    );
    cx.simulate_mouse_move(
        point(px(900.), start.y),
        MouseButton::Left,
        Modifiers::default(),
    );
    cx.simulate_mouse_up(
        point(px(900.), start.y),
        MouseButton::Left,
        Modifiers::default(),
    );
    draw(cx);
    // Dragging left widens it.
    let width = view.read_with(cx, |view, _| view.navigation_width.width(0.));
    assert!((width - 300.).abs() <= 1., "{width}");
}
