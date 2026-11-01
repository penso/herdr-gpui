//! The section drawn headless from fixture reads.
use super::*;
use crate::sidebar::layout_tests::{SidebarFixture, fixture_window, full_draw};
use gpui::{AppContext as _, Entity, Modifiers, VisualTestContext, px, size};

pub(super) fn window(
    cx: &mut gpui::TestAppContext,
) -> (Entity<HerdrWindow>, &mut VisualTestContext) {
    let (fixture, cx) = cx.add_window_view(|window, cx| {
        let view = cx.new(|cx| {
            let mut view = fixture_window(window, cx);
            view.config.sidebar_style.activity_bar = true;
            view
        });
        cx.observe(&view, |_, _, cx| cx.notify()).detach();
        SidebarFixture(view)
    });
    cx.simulate_resize(size(px(1100.), px(800.)));
    let view = cx.update(|_, cx| fixture.read(cx).0.clone());
    (view, cx)
}

pub(super) fn draw(cx: &mut VisualTestContext) {
    cx.update(|window, cx| {
        window.refresh();
        full_draw(window, cx).clear(cx);
    });
}

fn click(cx: &mut VisualTestContext, selector: &'static str) {
    let bounds = cx
        .debug_bounds(selector)
        .unwrap_or_else(|| panic!("no {selector}"));
    cx.simulate_click(bounds.center(), Modifiers::default());
    draw(cx);
}

/// Shows the section with two machines read: one with scripts, one
/// without the program.
fn shown(cx: &mut gpui::TestAppContext) -> (Entity<HerdrWindow>, &mut VisualTestContext) {
    let (view, cx) = window(cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.automations.install_for_test(
                wanted(Machine::Local, "This computer"),
                Read::Ready(Arc::new(status())),
            );
            view.automations
                .install_for_test(wanted(Machine::Ssh("pi".into()), "pi"), Read::Missing);
            view.show_activity_panel(ActivityPanel::Automations, window, cx);
        });
    });
    draw(cx);
    (view, cx)
}

#[gpui::test]
fn the_bar_lists_the_section_with_its_count(cx: &mut gpui::TestAppContext) {
    let (_, cx) = shown(cx);
    assert!(cx.debug_bounds("activity-automations").is_some());
    // Two scripts wait on the user: one to approve, one that failed.
    assert!(cx.debug_bounds("activity-automations-bubble").is_some());
    assert!(cx.debug_bounds("automations-section").is_some());
}

#[gpui::test]
fn hosts_show_a_card_each_with_a_row_per_script(cx: &mut gpui::TestAppContext) {
    let (_, cx) = shown(cx);
    let header = cx.debug_bounds("automations-header").unwrap();
    let first = cx.debug_bounds("automations-host-0").unwrap();
    let second = cx.debug_bounds("automations-host-1").unwrap();
    assert!(first.top() >= header.bottom());
    assert!(second.top() > first.bottom());
    assert!(cx.debug_bounds("automations-meter").is_some());
    // The host without the program offers to enable it; this one does not.
    assert!(cx.debug_bounds("automations-enable-1").is_some());
    assert!(cx.debug_bounds("automations-enable-0").is_none());
    assert!(cx.debug_bounds("automations-pause-all-0").is_some());
    assert!(cx.debug_bounds("automations-host-1-note").is_some());
    // Rows stay inside their card, however long the text.
    let row = cx.debug_bounds("automations-script-0-3").unwrap();
    assert!(row.left() >= first.left() && row.right() <= first.right());
}

#[gpui::test]
fn a_script_opens_beside_the_list_and_a_run_beside_it(cx: &mut gpui::TestAppContext) {
    let (view, cx) = shown(cx);
    // The running script, the host's first.
    click(cx, "automations-script-0-0");
    let list = cx.debug_bounds("automations-list").unwrap();
    let page = cx.debug_bounds("automations-page").unwrap();
    assert!(page.left() >= list.right());
    assert!(cx.debug_bounds("automations-script-page").is_some());
    assert!(cx.debug_bounds("automations-row-selected").is_some());
    assert!(cx.debug_bounds("automations-plan").is_some());
    // The running script can be stopped, and its runs open.
    assert!(cx.debug_bounds("automations-stop").is_some());
    view.update(cx, |view, cx| {
        let opened = Opened::Run {
            machine: Machine::Local,
            path: "/repo/.herdr/automations/nightly.luau".into(),
            run: 127,
        };
        // A page already read draws without asking the host.
        let detail = herdr_automation::report::RunDetail {
            run: run(127, "nightly", RunState::Completed, NOW - 33 * HOUR),
            steps: Vec::new(),
        };
        view.automations
            .runs
            .insert((Machine::Local, 127), Loaded::Read(Arc::new(detail)));
        view.automations.opened = Some(opened);
        cx.notify();
    });
    draw(cx);
    assert!(cx.debug_bounds("automations-run-page").is_some());
    assert!(cx.debug_bounds("automations-usage").is_some());
    assert!(cx.debug_bounds("automations-crumb-2").is_some());
    // The list folds away, and Close returns to the overview.
    click(cx, "automations-toggle-list");
    assert!(cx.debug_bounds("automations-list").is_none());
    click(cx, "automations-close");
    assert!(cx.debug_bounds("automations-page").is_none());
    assert!(cx.debug_bounds("automations-host-0").is_some());
}

#[gpui::test]
fn the_timeline_groups_runs_and_filters_by_host(cx: &mut gpui::TestAppContext) {
    let (view, cx) = shown(cx);
    click(cx, "automations-mode-timeline");
    assert!(cx.debug_bounds("automations-group-NeedsYou").is_some());
    assert!(cx.debug_bounds("automations-group-Now").is_some());
    assert!(cx.debug_bounds("automations-group-Older").is_some());
    assert!(cx.debug_bounds("automations-older").is_some());
    let needs = cx.debug_bounds("automations-group-NeedsYou").unwrap();
    let older = cx.debug_bounds("automations-group-Older").unwrap();
    assert!(needs.top() < older.top());
    // Only machines that were read have a chip.
    assert!(cx.debug_bounds("automations-filter-0").is_some());
    assert!(cx.debug_bounds("automations-filter-1").is_none());
    click(cx, "automations-filter-0");
    assert_eq!(
        view.read_with(cx, |view, _| view.automations.filter.clone()),
        Some(Machine::Local)
    );
}

#[gpui::test]
fn review_opens_the_approval_dialog_and_cancel_closes_it(cx: &mut gpui::TestAppContext) {
    let (view, cx) = shown(cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.review_automation(
                &Machine::Local,
                "/repo/.herdr/automations/deploy.luau",
                window,
                cx,
            );
        });
    });
    draw(cx);
    assert_eq!(
        view.read_with(cx, |view, _| view.menu.page),
        Some(crate::menu::Page::Automation)
    );
    assert!(cx.debug_bounds("automation-approve").is_some());
    assert!(cx.debug_bounds("automation-approve-plan").is_some());
    assert!(cx.debug_bounds("automation-approve-submit").is_some());
    click(cx, "automation-approve-cancel");
    view.read_with(cx, |view, _| {
        assert_eq!(view.menu.page, None);
        assert!(view.automations.dialog.is_none());
        // Nothing was asked of the host.
        assert!(
            view.automations
                .hosts
                .iter()
                .all(|host| host.busy.is_none())
        );
    });
}

#[gpui::test]
fn only_a_waiting_script_can_be_approved(cx: &mut gpui::TestAppContext) {
    let (view, cx) = shown(cx);
    view.read_with(cx, |view, _| {
        let approve = view
            .automation_approval(&Machine::Local, "/repo/.herdr/automations/deploy.luau")
            .unwrap();
        assert_eq!(approve.hash, "deploy-hash-0123456789abcdef");
        assert!(matches!(
            view.automation_approval(&Machine::Local, "/repo/.herdr/automations/nightly.luau"),
            Err(Error::Approved { .. })
        ));
        assert!(matches!(
            view.automation_approval(&Machine::Local, "/nope.luau"),
            Err(Error::NoScript { .. })
        ));
        assert!(matches!(
            view.automation_approval(&Machine::Ssh("pi".into()), "/x.luau"),
            Err(Error::NotRead { .. })
        ));
    });
}
