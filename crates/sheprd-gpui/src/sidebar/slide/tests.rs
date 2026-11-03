use super::*;
use crate::sidebar::{SidebarMode, layout_tests::fixture_window};
use gpui::TestAppContext;

#[gpui::test]
fn hiding_keeps_the_expanded_layout_until_the_sidebar_has_slid_off(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(fixture_window);
    let tick = |cx: &mut gpui::VisualTestContext| {
        view.update(cx, |view, _| {
            let visible = view.sidebar_visible;
            view.sidebar_slide.advance(visible, motion::now());
        });
    };
    view.update(cx, |view, _| {
        view.sidebar_slide.live = true;
        view.toggle_sidebar();
        assert!(!view.sidebar_visible);
    });
    tick(cx);
    view.read_with(cx, |view, _| {
        assert!(view.sidebar_slide.closing());
        assert_eq!(view.chrome_sidebar_mode(), SidebarMode::Expanded);
        let shown = view.sidebar_slide_shown().expect("it slides");
        assert!(shown > 0.9, "{shown}");
    });

    // Once it has slid off, the window takes the collapsed layout.
    view.update(cx, |view, _| {
        view.sidebar_slide.toggle.set(false, motion::now(), true);
    });
    tick(cx);
    view.read_with(cx, |view, _| {
        assert!(!view.sidebar_slide.closing());
        assert_ne!(view.chrome_sidebar_mode(), SidebarMode::Expanded);
        assert!(view.sidebar_slide_shown().is_none());
    });

    // Showing it lays it out at once and slides it in.
    view.update(cx, |view, _| view.toggle_sidebar());
    tick(cx);
    view.read_with(cx, |view, _| {
        assert!(!view.sidebar_slide.closing());
        assert_eq!(view.chrome_sidebar_mode(), SidebarMode::Expanded);
        let shown = view.sidebar_slide_shown().expect("it slides in");
        assert!(shown < 0.1, "{shown}");
    });
}

#[gpui::test]
fn a_sidebar_hidden_without_a_toggle_is_simply_gone(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(fixture_window);
    view.update(cx, |view, _| {
        view.sidebar_slide.live = true;
        view.sidebar_visible = false;
    });
    view.update(cx, |view, _| {
        view.sidebar_slide.advance(false, motion::now());
    });
    view.read_with(cx, |view, _| {
        assert!(!view.sidebar_slide.closing());
        assert!(view.sidebar_slide_shown().is_none());
    });
}
