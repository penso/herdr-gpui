//! A pane whose content runs past it shows a thumb that follows its scroll
//! and drags it; one that fits shows none.
use super::*;
use gpui::{px, size};

#[gpui::test]
fn overflowing_panes_show_a_thumb_that_drags_them(cx: &mut TestAppContext) {
    let main = cx.add_window(crate::sidebar::layout_tests::fixture_window);
    let weak = cx.update(|cx| main.update(cx, |_, _, cx| cx.weak_entity()).unwrap());
    let (view, cx) = cx.add_window_view(|_, cx| SettingsWindow::new(weak, cx));
    let draw = |cx: &mut VisualTestContext| {
        cx.update(|window, cx| crate::sidebar::layout_tests::full_draw(window, cx).clear(cx));
    };

    // Tall enough for every category: the section list has no thumb.
    cx.simulate_resize(size(px(900.), px(1400.)));
    draw(cx);
    draw(cx);
    assert!(cx.debug_bounds("settings-sections-thumb").is_none());

    cx.simulate_resize(size(px(680.), px(420.)));
    // The thumb reads the layout before it, so a resize shows it next frame.
    draw(cx);
    draw(cx);
    for (pane, area) in [
        ("settings-sections-thumb", "settings-sections"),
        ("settings-body-thumb", "settings-body"),
    ] {
        let thumb = cx.debug_bounds(pane).unwrap_or_else(|| panic!("{pane}"));
        let area = cx.debug_bounds(area).unwrap();
        assert!(thumb.size.height < area.size.height, "{pane}: {thumb:?}");
        assert!(
            (thumb.top() - area.top()).abs() < px(1.),
            "{pane} starts at the top"
        );
        assert!(thumb.right() <= area.right(), "{pane} sits inside the pane");
    }

    // Dragging the body's thumb down scrolls the body, and the thumb follows.
    let thumb = cx.debug_bounds("settings-body-thumb").unwrap();
    let from = thumb.center();
    let to = from + point(px(0.), px(120.));
    cx.simulate_mouse_down(from, MouseButton::Left, Default::default());
    cx.simulate_mouse_move(to, Some(MouseButton::Left), Default::default());
    cx.simulate_mouse_move(to, Some(MouseButton::Left), Default::default());
    cx.simulate_mouse_up(to, MouseButton::Left, Default::default());
    draw(cx);
    let scrolled = view.read_with(cx, |view, _| view.body_scroll.offset().y);
    assert!(scrolled < px(0.), "the body scrolled: {scrolled:?}");
    let moved = cx.debug_bounds("settings-body-thumb").unwrap();
    assert!(moved.top() > thumb.top(), "{moved:?} after {thumb:?}");
}
