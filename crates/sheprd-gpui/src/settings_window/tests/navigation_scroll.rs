//! A window too short for every category scrolls the list instead of
//! clipping the last ones, and every category stays reachable.
use super::*;
use gpui::{px, size};

#[gpui::test]
fn a_short_window_scrolls_the_category_list(cx: &mut TestAppContext) {
    let main = cx.add_window(crate::sidebar::layout_tests::fixture_window);
    let weak = cx.update(|cx| main.update(cx, |_, _, cx| cx.weak_entity()).unwrap());
    let (view, cx) = cx.add_window_view(|_, cx| SettingsWindow::new(weak, cx));
    cx.simulate_resize(size(px(680.), px(480.)));
    let draw = |cx: &mut VisualTestContext| {
        cx.update(|window, cx| crate::sidebar::layout_tests::full_draw(window, cx).clear(cx));
    };
    draw(cx);
    let last = Section::ALL.len() - 1;
    let id: &'static str = Section::ALL[last].selector().leak();
    let list = cx.debug_bounds("settings-sections").unwrap();
    let bottom = cx.debug_bounds(id).unwrap();
    assert!(list.bottom() <= px(480.), "{list:?}");
    assert!(bottom.bottom() > list.bottom(), "the fixture must overflow");

    view.update(cx, |view, cx| {
        view.navigation_scroll
            .scroll_to_item(Section::ALL[last].nav_item());
        cx.notify();
    });
    draw(cx);
    let list = cx.debug_bounds("settings-sections").unwrap();
    let bottom = cx.debug_bounds(id).unwrap();
    assert!(
        bottom.top() >= list.top() && bottom.bottom() <= list.bottom() + px(1.),
        "{bottom:?} in {list:?}"
    );
    cx.simulate_click(bottom.center(), Default::default());
    cx.run_until_parked();
    view.read_with(cx, |view, _| assert_eq!(view.section, Section::ALL[last]));
}
