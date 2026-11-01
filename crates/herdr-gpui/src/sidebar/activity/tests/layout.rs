use super::*;
use crate::sidebar::layout_tests::{SidebarFixture, fixture_window, full_draw};
use gpui::{AppContext as _, Entity, Modifiers, VisualTestContext, px, size};

fn fixture(cx: &mut gpui::TestAppContext) -> (Entity<HerdrWindow>, &mut VisualTestContext) {
    let (fixture, cx) = cx.add_window_view(|window, cx| {
        crate::bind_keys(cx);
        let view = cx.new(|cx| {
            let mut view = fixture_window(window, cx);
            view.config.sidebar_style.activity_bar = true;
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

fn click(cx: &mut VisualTestContext, selector: &'static str) {
    let bounds = cx.debug_bounds(selector).unwrap();
    cx.simulate_click(bounds.center(), Modifiers::default());
    draw(cx);
}

#[gpui::test]
fn the_bar_leads_the_expanded_sidebar_and_widens_its_column(cx: &mut gpui::TestAppContext) {
    let (view, cx) = fixture(cx);
    let bar = cx.debug_bounds("activity-bar").unwrap();
    let sidebar = cx.debug_bounds("sidebar").unwrap();
    assert_eq!(bar.size.width, px(BAR_WIDTH));
    assert_eq!(bar.left(), px(0.));
    // The panel keeps its own width beside the bar rather than giving it up.
    assert_eq!(sidebar.left(), bar.right());
    let expected = cx.update(|_, cx| {
        let view = view.read(cx);
        crate::sidebar::sidebar_width(view.sidebar_width, 1000.) + BAR_WIDTH
    });
    assert_eq!(
        cx.update(|_, cx| view.read(cx).sidebar_column_width(1000.)),
        Some(expected)
    );
    // Settings moves to the bar; the footer keeps the picker.
    assert!(cx.debug_bounds("activity-settings").is_some());
    assert!(cx.debug_bounds("device-settings").is_none());
    assert!(cx.debug_bounds("device-footer-initials").is_some());
}

#[gpui::test]
fn a_section_takes_the_body_beside_the_bar_and_workspaces_brings_it_back(
    cx: &mut gpui::TestAppContext,
) {
    let (view, cx) = fixture(cx);
    let terminal_column = cx.debug_bounds("sidebar").unwrap().right();
    click(cx, "activity-pull-requests");
    assert_eq!(
        cx.update(|_, cx| view.read(cx).activity.panel),
        ActivityPanel::PullRequests
    );
    // The bar stays; the sidebar and the terminals give way to the section.
    let bar = cx.debug_bounds("activity-bar").unwrap();
    let section = cx.debug_bounds("activity-section").unwrap();
    assert!(cx.debug_bounds("sidebar").is_none());
    assert_eq!(section.left(), bar.right());
    assert!(section.left() < terminal_column);
    assert_eq!(
        cx.update(|_, cx| view.read(cx).sidebar_column_width(1000.)),
        Some(BAR_WIDTH)
    );
    // The status bar stays under it.
    let status = cx.debug_bounds("connection-status").unwrap();
    assert!(status.top() >= section.bottom());
    // The fixture's socket cannot run the orchestrator's scripts, so the
    // list has no repository, which its empty state says.
    assert!(cx.debug_bounds("orchestrator-list").is_some());
    // The overview takes the body as the other sections do.
    click(cx, "activity-overview");
    assert_eq!(
        cx.update(|_, cx| view.read(cx).activity.panel),
        ActivityPanel::Overview
    );
    assert!(cx.debug_bounds("overview").is_some());
    assert!(cx.debug_bounds("activity-section-empty").is_none());
    click(cx, "activity-pull-requests");
    // A section picked again stays.
    click(cx, "activity-pull-requests");
    assert!(cx.debug_bounds("activity-section").is_some());
    click(cx, "activity-workspaces");
    assert!(cx.debug_bounds("activity-section").is_none());
    assert_eq!(
        cx.debug_bounds("sidebar").unwrap().left(),
        cx.debug_bounds("activity-bar").unwrap().right()
    );
    // Workspaces picked again collapses the sidebar to the rail, which the
    // bar keeps leading.
    click(cx, "activity-workspaces");
    cx.update(|_, cx| assert!(!view.read(cx).sidebar_visible));
    assert_eq!(
        cx.debug_bounds("sidebar-rail").unwrap().left(),
        cx.debug_bounds("activity-bar").unwrap().right()
    );
}

#[gpui::test]
fn zen_mode_keeps_the_terminals_whatever_section_was_shown(cx: &mut gpui::TestAppContext) {
    let (view, cx) = fixture(cx);
    click(cx, "activity-issues");
    assert!(view.read_with(cx, |view, _| view.activity_section().is_some()));
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.command(crate::controls::Command::ToggleZenMode, window, cx)
        })
    });
    assert!(view.read_with(cx, |view, _| view.activity_section().is_none()));
}

#[gpui::test]
fn turning_the_bar_off_restores_the_plain_sidebar(cx: &mut gpui::TestAppContext) {
    let (view, cx) = fixture(cx);
    view.update(cx, |view, cx| {
        // A stored section waits for the bar; the terminals show meanwhile.
        view.activity.panel = ActivityPanel::Issues;
        view.config.sidebar_style.activity_bar = false;
        cx.notify();
    });
    draw(cx);
    assert!(view.read_with(cx, |view, _| view.activity_section().is_none()));
    assert!(cx.debug_bounds("activity-section").is_none());
    assert!(cx.debug_bounds("activity-bar").is_none());
    assert_eq!(cx.debug_bounds("sidebar").unwrap().left(), px(0.));
    assert!(cx.debug_bounds("device-settings").is_some());
    let width = cx.update(|_, cx| view.read(cx).sidebar_column_width(1000.));
    assert_eq!(width, Some(crate::sidebar::sidebar_width(None, 1000.)));
}

#[test]
fn panel_names_round_trip_and_are_unique() {
    let mut seen = std::collections::HashSet::new();
    for &panel in ActivityPanel::ALL {
        assert!(seen.insert(panel.name()));
        assert_eq!(ActivityPanel::parse(panel.name()), Some(panel));
        let json = serde_json::to_string(&panel).unwrap();
        assert_eq!(json, format!("\"{}\"", panel.name()));
    }
    assert_eq!(ActivityPanel::parse("dashboard"), None);
}

#[gpui::test]
fn a_stored_panel_yields_to_one_picked_before_it_loaded(cx: &mut gpui::TestAppContext) {
    let (view, cx) = fixture(cx);
    view.update(cx, |view, _| {
        assert_eq!(view.activity.chosen(), None);
        view.activity
            .restore(Some(ActivityPanel::Usage), &Default::default());
        assert_eq!(view.activity.panel, ActivityPanel::Usage);
    });
    let (view, cx) = fixture(cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.show_activity_panel(ActivityPanel::Issues, window, cx);
            view.activity
                .restore(Some(ActivityPanel::Usage), &Default::default());
            assert_eq!(view.activity.panel, ActivityPanel::Issues);
            assert_eq!(view.activity.chosen(), Some(ActivityPanel::Issues));
        })
    });
}
