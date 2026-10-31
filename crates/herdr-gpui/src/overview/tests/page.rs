use crate::{
    HerdrWindow,
    overview::State,
    sidebar::{
        ActivityPanel,
        layout_tests::{SidebarFixture, fixture_window, full_draw, snapshot},
    },
    state::ConnectionStatus,
};
use gpui::{AppContext as _, Bounds, Entity, Modifiers, Pixels, VisualTestContext, px, size};
use herdr_client::protocol::AgentStatus;
use std::sync::Arc;

/// A connected window with the bar, a blocked, a working, and a finished
/// agent, on
/// the Overview.
pub(super) fn fixture(
    cx: &mut gpui::TestAppContext,
) -> (Entity<HerdrWindow>, &mut VisualTestContext) {
    let (view, cx) = window(cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.show_activity_panel(ActivityPanel::Overview, window, cx);
            view.poll_overview(cx);
        })
    });
    draw(cx);
    (view, cx)
}

/// The fixture's window before it shows the Overview.
pub(super) fn window(
    cx: &mut gpui::TestAppContext,
) -> (Entity<HerdrWindow>, &mut VisualTestContext) {
    let (fixture, cx) = cx.add_window_view(|window, cx| {
        crate::bind_keys(cx);
        let view = cx.new(|cx| {
            let mut view = fixture_window(window, cx);
            view.config.sidebar_style.activity_bar = true;
            let mut shown = snapshot(2);
            shown.focused_workspace_id = Some("w0".into());
            shown.focused_tab_id = Some("t0".into());
            shown.agents[0].agent_status = AgentStatus::Blocked;
            let mut done = shown.agents[0].clone();
            done.pane_id = "p2".into();
            done.name = Some("finished".into());
            done.display_agent = None;
            done.agent_status = AgentStatus::Done;
            shown.agents.push(done);
            view.live.snapshot = Some(Arc::new(shown));
            view.live.status = ConnectionStatus::Connected;
            view
        });
        cx.observe(&view, |_, _, cx| cx.notify()).detach();
        SidebarFixture(view)
    });
    cx.simulate_resize(size(px(1200.), px(800.)));
    let view = cx.update(|_, cx| fixture.read(cx).0.clone());
    (view, cx)
}

pub(super) fn draw(cx: &mut VisualTestContext) {
    cx.update(|window, cx| {
        window.refresh();
        full_draw(window, cx).clear(cx);
    });
}

/// The bounds an element drew with; selectors built at run time are leaked,
/// as the test context keeps only static ones.
pub(super) fn bounds(
    cx: &mut VisualTestContext,
    selector: impl Into<String>,
) -> Option<Bounds<Pixels>> {
    let selector: &'static str = Box::leak(selector.into().into_boxed_str());
    cx.debug_bounds(selector)
}

pub(super) fn click(cx: &mut VisualTestContext, selector: impl Into<String>) {
    let selector = selector.into();
    let bounds = bounds(cx, selector.clone()).unwrap_or_else(|| panic!("{selector}"));
    cx.simulate_click(bounds.center(), Modifiers::default());
    draw(cx);
}

/// The selector part of the row whose agent is in `state`.
pub(super) fn row(view: &Entity<HerdrWindow>, cx: &mut VisualTestContext, state: State) -> String {
    cx.update(|_, cx| {
        view.read(cx)
            .overview_rows(cx)
            .into_iter()
            .find(|row| row.state == state)
            .map(|row| row.key.selector())
            .unwrap()
    })
}

#[gpui::test]
fn the_overview_leads_with_what_needs_the_user(cx: &mut gpui::TestAppContext) {
    let (view, cx) = fixture(cx);
    let page = cx.debug_bounds("overview").unwrap();
    // It takes the body beside the bar, with the status bar under it.
    assert_eq!(
        page.left(),
        cx.debug_bounds("activity-bar").unwrap().right()
    );
    assert!(cx.debug_bounds("connection-status").unwrap().top() >= page.bottom());
    // A worktree build's banner sits at the window's foot here too.
    if let Some(banner) = cx.debug_bounds("worktree-banner") {
        assert!(banner.top() >= cx.debug_bounds("connection-status").unwrap().bottom());
        assert!(banner.top() > page.top());
    }
    let needs = cx.debug_bounds("overview-needs").unwrap();
    let (blocked, working) = (
        row(&view, cx, State::Blocked),
        row(&view, cx, State::Working),
    );
    let blocked = bounds(cx, format!("overview-row-{blocked}")).unwrap();
    let working = bounds(cx, format!("overview-row-{working}")).unwrap();
    assert!(needs.contains(&blocked.center()));
    assert!(!needs.contains(&working.center()));
    assert!(working.top() > needs.bottom());
    // Finished work, which waits on the user, comes before working agents.
    let done = row(&view, cx, State::Done);
    let ready = bounds(cx, format!("overview-row-{done}")).unwrap();
    assert!(ready.top() > needs.bottom() && ready.bottom() <= working.top());
    // The host column sits beside the list.
    assert!(cx.debug_bounds("overview-hosts").unwrap().left() >= blocked.right());
    // The bar counts what needs the user, and its icon pulses while an
    // agent works.
    assert_eq!(cx.update(|_, cx| view.read(cx).overview_needing()), 1);
    assert_eq!(cx.update(|_, cx| view.read(cx).overview_working()), 1);
}

#[gpui::test]
fn settling_a_row_moves_it_to_the_settled_shelf_and_back(cx: &mut gpui::TestAppContext) {
    let (view, cx) = fixture(cx);
    let blocked = row(&view, cx, State::Blocked);
    click(cx, format!("overview-settle-{blocked}"));
    let row_selector = format!("overview-row-{blocked}");
    assert!(bounds(cx, row_selector.clone()).is_none());
    cx.update(|_, cx| view.update(cx, |view, cx| view.poll_overview(cx)));
    assert_eq!(cx.update(|_, cx| view.read(cx).overview_needing()), 0);

    click(cx, "overview-settled");
    assert!(bounds(cx, row_selector.clone()).is_some());
    click(cx, format!("overview-restore-{blocked}"));
    let needs = cx.debug_bounds("overview-needs").unwrap();
    assert!(needs.contains(&bounds(cx, row_selector.clone()).unwrap().center()));
}

#[gpui::test]
fn a_snooze_preset_holds_the_row_until_its_time(cx: &mut gpui::TestAppContext) {
    let (view, cx) = fixture(cx);
    let blocked = row(&view, cx, State::Blocked);
    click(cx, format!("overview-snooze-{blocked}"));
    assert!(cx.debug_bounds("overview-snooze-menu").is_some());
    click(cx, "overview-snooze-preset-0");
    assert!(cx.debug_bounds("overview-snooze-menu").is_none());
    assert!(bounds(cx, format!("overview-row-{blocked}")).is_none());
    let sections = cx.update(|_, cx| view.read(cx).overview_sections(cx));
    assert_eq!(sections.snoozed.len(), 1);
    assert!(sections.needs.is_empty());
}

#[gpui::test]
fn pinning_a_row_leads_the_list_with_it(cx: &mut gpui::TestAppContext) {
    let (view, cx) = fixture(cx);
    let working = row(&view, cx, State::Working);
    click(cx, format!("overview-pin-{working}"));
    let pinned = bounds(cx, format!("overview-row-{working}")).unwrap();
    assert!(pinned.bottom() <= cx.debug_bounds("overview-needs").unwrap().top());
}

#[gpui::test]
fn the_host_column_and_each_host_fold(cx: &mut gpui::TestAppContext) {
    let (view, cx) = fixture(cx);
    let host = cx.update(|_, cx| view.read(cx).endpoints[0].id.clone());
    let card = format!("overview-host-card-{host}");
    let open = bounds(cx, card.clone()).unwrap();
    click(cx, format!("overview-host-{host}"));
    // Folded, the host is its header line alone.
    let folded = bounds(cx, card.clone()).unwrap();
    assert!(folded.size.height < open.size.height);
    assert!(
        folded.size.height
            <= bounds(cx, format!("overview-host-{host}"))
                .unwrap()
                .size
                .height
    );
    let list = cx.debug_bounds("overview-list").unwrap();
    click(cx, "overview-hosts-toggle");
    assert!(cx.debug_bounds("overview-hosts").is_none());
    assert!(cx.debug_bounds("overview-hosts-strip").is_some());
    // The list takes the room the column gave up.
    assert!(cx.debug_bounds("overview-list").unwrap().size.width > list.size.width);
}

#[gpui::test]
fn opening_a_row_shows_its_agent_among_the_terminals(cx: &mut gpui::TestAppContext) {
    let (view, cx) = fixture(cx);
    let blocked = row(&view, cx, State::Blocked);
    click(cx, format!("overview-row-{blocked}"));
    assert_eq!(
        cx.update(|_, cx| view.read(cx).activity.panel),
        ActivityPanel::Workspaces
    );
    assert!(cx.debug_bounds("overview").is_none());
}

#[gpui::test]
fn a_narrow_window_folds_the_hosts_and_keeps_titles_readable(cx: &mut gpui::TestAppContext) {
    let (view, cx) = fixture(cx);
    cx.simulate_resize(size(px(760.), px(700.)));
    draw(cx);
    assert!(cx.debug_bounds("overview-hosts").is_none());
    assert!(cx.debug_bounds("overview-hosts-strip").is_some());
    let blocked = row(&view, cx, State::Blocked);
    let title = bounds(cx, format!("overview-title-{blocked}")).unwrap();
    assert!(title.size.width >= px(140.), "{title:?}");
    // The row's last column still ends inside the list.
    let row = bounds(cx, format!("overview-row-{blocked}")).unwrap();
    assert!(row.right() <= cx.debug_bounds("overview-list").unwrap().right());
    // The user can still open the column there.
    click(cx, "overview-hosts-toggle");
    assert!(cx.debug_bounds("overview-hosts").is_some());
}

#[gpui::test]
fn nothing_shows_for_groups_with_nothing_in_them(cx: &mut gpui::TestAppContext) {
    let (view, cx) = fixture(cx);
    let blocked = row(&view, cx, State::Blocked);
    click(cx, format!("overview-settle-{blocked}"));
    // Nothing needs the user now: no card says so.
    assert!(cx.debug_bounds("overview-needs").is_none());
    assert!(cx.debug_bounds("overview").is_some());
}
