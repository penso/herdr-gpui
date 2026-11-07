use super::*;
use crate::sidebar::layout_tests::{SidebarFixture, fixture_window, full_draw};
use gpui::{AppContext as _, Bounds, Entity, Modifiers, Pixels, VisualTestContext, px, size};

fn fixture<'a>(
    cx: &'a mut gpui::TestAppContext,
    collapsed_mode: &str,
) -> (Entity<SheprdWindow>, &'a mut VisualTestContext) {
    let settings = format!("[ui]\nsidebar_collapsed_mode = '{collapsed_mode}'");
    let (fixture, cx) = cx.add_window_view(move |window, cx| {
        let view = cx.new(|cx| {
            let mut view = fixture_window(window, cx);
            view.config.sidebar_style.activity_bar = true;
            view.settings.shared =
                Some(crate::herdr_settings::Settings::parse_text(&settings).unwrap());
            view
        });
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

fn toggle(cx: &mut VisualTestContext) -> Bounds<Pixels> {
    let bounds = cx.debug_bounds("toggle-sidebar").unwrap();
    cx.simulate_click(bounds.center(), Modifiers::default());
    draw(cx);
    bounds
}

fn inside(outer: Bounds<Pixels>, inner: Bounds<Pixels>) -> bool {
    outer.contains(&inner.origin)
        && inner.right() <= outer.right()
        && inner.bottom() <= outer.bottom()
}

#[gpui::test]
fn the_toggle_ends_the_workspaces_title_beside_the_list(cx: &mut gpui::TestAppContext) {
    let (_, cx) = fixture(cx, "compact");
    let title = cx.debug_bounds("activity-title").unwrap();
    let sidebar = cx.debug_bounds("sidebar").unwrap();
    let button = cx.debug_bounds("toggle-sidebar").unwrap();
    // Over the list, at its trailing edge, not in the window's header.
    assert!(inside(title, button));
    assert_eq!(title.left(), sidebar.left());
    assert!(button.right() <= sidebar.right());
    assert!(button.left() > sidebar.center().x);
    assert!(button.top() >= title.top());
    let strip = cx.debug_bounds("strip-titlebar-leading");
    assert!(strip.is_none_or(|strip| !strip.contains(&button.center())));
}

#[gpui::test]
fn collapsing_keeps_the_bar_with_the_rail_and_toggle_beside_it(cx: &mut gpui::TestAppContext) {
    let (view, cx) = fixture(cx, "compact");
    toggle(cx);
    assert!(!view.read_with(cx, |view, _| view.sidebar_visible));
    let bar = cx.debug_bounds("activity-bar").unwrap();
    let rail = cx.debug_bounds("sidebar-rail").unwrap();
    let title = cx.debug_bounds("activity-title").unwrap();
    let button = cx.debug_bounds("toggle-sidebar").unwrap();
    assert_eq!(bar.left(), px(0.));
    assert_eq!(rail.left(), bar.right());
    // The toggle tops the rail, so it brings the list back from where it went.
    assert_eq!(title.left(), rail.left());
    assert_eq!(title.size.width, rail.size.width);
    assert_eq!(title.bottom(), rail.top());
    assert!(inside(title, button));
    let width = view.read_with(cx, |view, _| view.sidebar_column_width(1000.));
    assert_eq!(width, Some(BAR_WIDTH + f32::from(rail.size.width)));
    toggle(cx);
    assert!(view.read_with(cx, |view, _| view.sidebar_visible));
    assert!(cx.debug_bounds("sidebar-rail").is_none());
    assert_eq!(
        cx.debug_bounds("sidebar").unwrap().left(),
        cx.debug_bounds("activity-bar").unwrap().right()
    );
}

#[gpui::test]
fn a_hidden_sidebar_keeps_the_bar_whose_workspaces_brings_it_back(cx: &mut gpui::TestAppContext) {
    let (view, cx) = fixture(cx, "hidden");
    toggle(cx);
    assert!(!view.read_with(cx, |view, _| view.sidebar_visible));
    assert!(cx.debug_bounds("activity-bar").is_some());
    assert!(cx.debug_bounds("sidebar-rail").is_none());
    assert!(cx.debug_bounds("activity-title").is_none());
    assert_eq!(
        view.read_with(cx, |view, _| view.sidebar_column_width(1000.)),
        Some(BAR_WIDTH)
    );
    let workspaces = cx.debug_bounds("activity-workspaces").unwrap();
    cx.simulate_click(workspaces.center(), Modifiers::default());
    draw(cx);
    assert!(view.read_with(cx, |view, _| view.sidebar_visible));
    assert!(cx.debug_bounds("toggle-sidebar").is_some());
}

#[gpui::test]
fn zen_mode_sets_the_bar_aside_and_hides_the_column(cx: &mut gpui::TestAppContext) {
    let (view, cx) = fixture(cx, "compact");
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.command(crate::controls::Command::ToggleZenMode, window, cx)
        })
    });
    assert!(!view.read_with(cx, |view, _| view.activity_bar_leads()));
    assert_eq!(
        view.read_with(cx, |view, _| view.sidebar_column_width(1000.)),
        None
    );
}

#[gpui::test]
fn the_full_width_header_leaves_the_toggle_to_the_column(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(|window, cx| {
        // Herdr's tab bar at the bottom keeps the full-width header.
        let mut view = fixture_window(window, cx);
        view.config.sidebar_style.activity_bar = true;
        view.settings.shared = Some(
            crate::herdr_settings::Settings::parse_text("[ui]\ntab_bar_position = 'bottom'")
                .unwrap(),
        );
        view
    });
    cx.simulate_resize(size(px(1200.), px(600.)));
    for visible in [true, false] {
        cx.update(|window, cx| {
            window.refresh();
            window.draw(cx).clear(cx);
        });
        assert_eq!(view.read_with(cx, |view, _| view.sidebar_visible), visible);
        let header = cx.debug_bounds("titlebar").unwrap();
        let title = cx.debug_bounds("activity-title").unwrap();
        let button = cx.debug_bounds("toggle-sidebar").unwrap();
        assert!(inside(title, button));
        assert!(button.top() >= header.bottom());
        // Back still follows the traffic lights' clearance in the header.
        assert!(header.contains(&cx.debug_bounds("titlebar-back").unwrap().center()));
        cx.simulate_click(button.center(), Modifiers::default());
    }
    assert!(view.read_with(cx, |view, _| view.sidebar_visible));
}
