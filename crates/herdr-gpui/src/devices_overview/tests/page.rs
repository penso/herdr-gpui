use super::*;
use crate::browser::{Location, Store};

fn overview_tabs(view: &Entity<HerdrWindow>, cx: &mut VisualTestContext) -> usize {
    cx.update(|_, cx| {
        let scope = crate::browser::scope(&view.read(cx).endpoints[0]);
        cx.try_global::<Store>().map_or(0, |store| {
            store
                .in_workspace(&scope, "w0")
                .filter(|tab| tab.location == Some(Location::Devices))
                .count()
        })
    })
}

#[gpui::test]
fn the_overview_opens_once_as_a_tab_and_lists_each_device(cx: &mut gpui::TestAppContext) {
    let (view, cx) = window(cx);
    draw(cx);
    assert!(cx.debug_bounds("devices-tab").is_none());

    open(&view, cx);
    assert_eq!(overview_tabs(&view, cx), 1);
    let tab = cx.debug_bounds("devices-tab").unwrap();
    for part in [
        "devices-activity",
        "devices-totals",
        "devices-table",
        "devices-add",
    ] {
        let bounds = cx.debug_bounds(part).unwrap_or_else(|| panic!("{part}"));
        assert!(tab.contains(&bounds.origin), "{part} lies in the tab");
    }
    assert!(cx.debug_bounds("devices-row-local").is_some());
    assert!(view.read_with(cx, |view, _| view.devices_overview_open()));

    // Opening it again brings back the same tab.
    open(&view, cx);
    assert_eq!(overview_tabs(&view, cx), 1);
}

#[gpui::test]
fn the_tick_records_each_connected_devices_agents(cx: &mut gpui::TestAppContext) {
    let (view, cx) = window(cx);
    open(&view, cx);
    let devices = view.read_with(cx, |view, _| view.overview_devices());
    assert_eq!(devices.len(), 1);
    assert_eq!(devices[0].link, crate::devices_overview::Link::Online);
    assert_eq!(devices[0].tally.working, 2);
    cx.update(|_, cx| view.update(cx, |view, cx| view.poll_devices_overview(cx)));
    let lane = view.read_with(cx, |view, _| view.devices_overview.history.lane("local"));
    assert_eq!(lane.last().copied().flatten().map(|c| c.working), Some(2));
}

#[gpui::test]
fn view_all_devices_opens_a_lane_per_device(cx: &mut gpui::TestAppContext) {
    let (view, cx) = window(cx);
    open(&view, cx);
    let closed = cx.debug_bounds("devices-activity").unwrap();
    let toggle = cx.debug_bounds("devices-lanes-toggle").unwrap();
    cx.simulate_click(toggle.center(), gpui::Modifiers::default());
    draw(cx);
    let open = cx.debug_bounds("devices-activity").unwrap();
    assert!(open.size.height > closed.size.height, "the lanes take room");
    let id = view.read_with(cx, |view, _| {
        *view.devices_overview.pages.keys().next().unwrap()
    });
    assert!(view.read_with(cx, |view, _| view.devices_overview.pages[&id].lanes));
}

#[gpui::test]
fn closing_the_tab_drops_its_page(cx: &mut gpui::TestAppContext) {
    let (view, cx) = window(cx);
    open(&view, cx);
    let id = view.read_with(cx, |view, _| {
        *view.devices_overview.pages.keys().next().unwrap()
    });
    cx.update(|_, cx| Store::update(cx, |store| store.close(id)));
    cx.update(|_, cx| view.update(cx, |view, cx| view.poll_devices_overview(cx)));
    assert!(!view.read_with(cx, |view, _| view.devices_overview_open()));
}

#[gpui::test]
fn a_narrow_tab_wraps_the_load_columns_instead_of_cutting_them(cx: &mut gpui::TestAppContext) {
    let (view, cx) = window(cx);
    cx.simulate_resize(gpui::size(gpui::px(1400.), gpui::px(900.)));
    open(&view, cx);
    let wide_row = cx.debug_bounds("devices-row-local").unwrap();
    let wide_totals = cx.debug_bounds("devices-totals").unwrap();
    let wide_load = cx.debug_bounds("devices-load-local").unwrap();
    assert!(
        wide_load.top() < wide_row.top() + (wide_row.size.height / 2.),
        "one line when there is room"
    );

    cx.simulate_resize(gpui::size(gpui::px(820.), gpui::px(900.)));
    draw(cx);
    let tab = cx.debug_bounds("devices-tab").unwrap();
    let row = cx.debug_bounds("devices-row-local").unwrap();
    let load = cx.debug_bounds("devices-load-local").unwrap();
    let header = cx.debug_bounds("devices-load-header").unwrap();
    assert!(load.right() <= tab.right(), "{load:?} inside {tab:?}");
    assert!(row.size.height > wide_row.size.height, "the load wrapped");
    let totals = cx.debug_bounds("devices-totals").unwrap();
    assert_eq!(
        totals.size.height, wide_totals.size.height,
        "the four totals keep one row"
    );
    assert!(
        (header.left() - load.left()).abs() < gpui::px(1.),
        "the header wraps in step: {header:?} {load:?}"
    );
}

#[gpui::test]
fn the_device_pickers_activity_card_opens_the_overview(cx: &mut gpui::TestAppContext) {
    let (view, cx) = window(cx);
    cx.simulate_resize(gpui::size(gpui::px(1000.), gpui::px(800.)));
    draw(cx);
    let picker = cx.debug_bounds("device-picker").unwrap().center();
    cx.simulate_click(picker, gpui::Modifiers::default());
    draw(cx);
    let card = cx.debug_bounds("device-activity").unwrap();
    let heading = cx.debug_bounds("device-row-0").unwrap();
    assert!(
        card.bottom() <= heading.top(),
        "the card sits above the list"
    );
    // Within the room the picker's list gives up for it (100 px with its
    // 4 px margins) at the default font size.
    assert!(card.size.height <= gpui::px(92.), "{card:?}");

    cx.simulate_click(card.center(), gpui::Modifiers::default());
    draw(cx);
    assert_eq!(view.read_with(cx, |view, _| view.menu.page), None);
    assert_eq!(overview_tabs(&view, cx), 1);
    assert!(cx.debug_bounds("devices-tab").is_some());
}

#[gpui::test]
fn clicking_a_device_row_opens_and_closes_its_agents(cx: &mut gpui::TestAppContext) {
    let (view, cx) = window(cx);
    cx.simulate_resize(gpui::size(gpui::px(1400.), gpui::px(900.)));
    open(&view, cx);
    assert!(cx.debug_bounds("devices-agents-local").is_none());

    let row = cx.debug_bounds("devices-row-local").unwrap();
    cx.simulate_click(row.center(), gpui::Modifiers::default());
    draw(cx);
    let list = cx.debug_bounds("devices-agents-local").unwrap();
    assert!(list.top() >= row.bottom(), "the agents open under the row");
    let first = cx.debug_bounds("devices-agent-local-p0").unwrap();
    let second = cx.debug_bounds("devices-agent-local-p1").unwrap();
    assert!(first.bottom() <= second.top());
    assert!(list.contains(&first.origin) && list.contains(&second.origin));

    cx.simulate_click(row.center(), gpui::Modifiers::default());
    draw(cx);
    assert!(cx.debug_bounds("devices-agents-local").is_none());
}

/// An agent in the tab the daemon already focuses moves no focus, so the
/// click itself must take the group back from the overview to the terminal.
#[gpui::test]
fn clicking_an_agent_in_the_focused_tab_shows_its_terminal(cx: &mut gpui::TestAppContext) {
    let (view, cx) = window(cx);
    cx.simulate_resize(gpui::size(gpui::px(1400.), gpui::px(900.)));
    open(&view, cx);
    let row = cx.debug_bounds("devices-row-local").unwrap();
    cx.simulate_click(row.center(), gpui::Modifiers::default());
    draw(cx);
    // `p0` sits in `t0`, the tab the fixture's daemon focuses.
    let agent = cx.debug_bounds("devices-agent-local-p0").unwrap();
    cx.simulate_click(agent.center(), gpui::Modifiers::default());
    draw(cx);
    assert!(
        cx.debug_bounds("devices-tab").is_none(),
        "the overview gave way"
    );
    assert!(cx.debug_bounds("terminal").is_some(), "its terminal shows");
    assert_eq!(overview_tabs(&view, cx), 1, "the overview tab stays open");
}
