//! The activity bar's Usage section against a headless window.
use crate::{
    SheprdWindow,
    sidebar::{
        ActivityPanel,
        layout_tests::{SidebarFixture, fixture_window, full_draw},
    },
};
use gpui::{AppContext as _, Entity, VisualTestContext, px, size};

fn fixture(
    cx: &mut gpui::TestAppContext,
    show: bool,
) -> (Entity<SheprdWindow>, &mut VisualTestContext) {
    let (fixture, cx) = cx.add_window_view(|window, cx| {
        let view = cx.new(|cx| {
            let mut view = fixture_window(window, cx);
            view.config.sidebar_style.activity_bar = true;
            view.config.usage.show = show;
            view
        });
        cx.observe(&view, |_, _, cx| cx.notify()).detach();
        SidebarFixture(view)
    });
    cx.simulate_resize(size(px(1100.), px(700.)));
    let view = cx.update(|_, cx| fixture.read(cx).0.clone());
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.show_activity_panel(ActivityPanel::Usage, window, cx)
        });
        window.refresh();
        full_draw(window, cx).clear(cx);
    });
    (view, cx)
}

#[gpui::test]
fn usage_lists_each_host_across_the_window(cx: &mut gpui::TestAppContext) {
    let (_, cx) = fixture(cx, true);
    let section = cx.debug_bounds("usage-section").unwrap();
    assert_eq!(
        section.left(),
        cx.debug_bounds("activity-bar").unwrap().right()
    );
    assert!(cx.debug_bounds("usage-section-refresh").is_some());
    let host = cx.debug_bounds("usage-section-host-0").unwrap();
    assert!(section.contains(&host.origin));
    assert!(cx.debug_bounds("usage-section-off").is_none());
}

#[gpui::test]
fn usage_turned_off_says_so_instead_of_listing_hosts(cx: &mut gpui::TestAppContext) {
    let (_, cx) = fixture(cx, false);
    assert!(cx.debug_bounds("usage-section-off").is_some());
    assert!(cx.debug_bounds("usage-section-host-0").is_none());
    assert!(cx.debug_bounds("usage-section-refresh").is_none());
}
