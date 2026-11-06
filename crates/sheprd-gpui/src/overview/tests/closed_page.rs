use super::page::{bounds, click, draw, fixture, row};
use crate::{SheprdWindow, overview::State, sidebar::ActivityPanel};
use gpui::{Entity, VisualTestContext};
use std::sync::Arc;

/// Takes the working agent off the host, as closing its pane does, and
/// returns its active row's selector part and its closed id.
pub(super) fn close_working(
    view: &Entity<SheprdWindow>,
    cx: &mut VisualTestContext,
) -> (String, u64) {
    let working = row(view, cx, State::Working);
    let id = cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            let mut shown = view.live.snapshot.as_deref().unwrap().clone();
            shown.agents.retain(|agent| agent.pane_id != "p1");
            shown.revision += 1;
            view.live.snapshot = Some(Arc::new(shown));
            view.poll_overview(cx);
            view.overview.closed.agents().next().unwrap().id
        })
    });
    draw(cx);
    (working, id)
}

#[gpui::test]
fn a_closed_agent_waits_under_closed(cx: &mut gpui::TestAppContext) {
    let (view, cx) = fixture(cx);
    // Empty shelves do not show.
    assert!(cx.debug_bounds("overview-closed").is_none());
    assert!(cx.debug_bounds("overview-snoozed").is_none());
    assert!(cx.debug_bounds("overview-settled").is_none());
    let (working, id) = close_working(&view, cx);
    assert!(bounds(cx, format!("overview-row-{working}")).is_none());
    assert!(cx.debug_bounds("overview-closed").is_some());
    // Folded until asked for.
    assert!(bounds(cx, format!("overview-closed-{id}")).is_none());
    click(cx, "overview-closed");
    assert!(bounds(cx, format!("overview-closed-{id}")).is_some());
    assert!(bounds(cx, format!("overview-reopen-{id}")).is_some());

    click(cx, format!("overview-forget-{id}"));
    assert!(bounds(cx, format!("overview-closed-{id}")).is_none());
    assert!(cx.debug_bounds("overview-closed").is_none());
}

#[gpui::test]
fn reopening_shows_the_terminals_and_says_when_it_cannot(cx: &mut gpui::TestAppContext) {
    let (view, cx) = fixture(cx);
    let (_, id) = close_working(&view, cx);
    click(cx, "overview-closed");
    click(cx, format!("overview-reopen-{id}"));
    assert_eq!(
        cx.update(|_, cx| view.read(cx).activity.panel),
        ActivityPanel::Workspaces
    );
    // The fixture has no daemon to make the tab: the agent stays closed and
    // the window says why.
    let flash = cx.update(|_, cx| view.read(cx).flash.clone()).unwrap().0;
    assert!(flash.text.contains("did not reopen"), "{}", flash.text);
    assert!(!cx.update(|_, cx| view.read(cx).overview_reopening()));
    let closed = cx.update(|_, cx| view.read(cx).overview.closed.len());
    assert_eq!(closed, 1);
}

#[gpui::test]
fn with_nothing_to_list_the_page_says_what_it_is_for(cx: &mut gpui::TestAppContext) {
    let (view, cx) = fixture(cx);
    assert!(cx.debug_bounds("overview-empty").is_none());
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            // A new boot: its agents went with the old daemon, not closed.
            let mut shown = view.live.snapshot.as_deref().unwrap().clone();
            shown.agents.clear();
            shown.boot_id = "restarted".into();
            view.live.snapshot = Some(Arc::new(shown));
            view.poll_overview(cx);
        })
    });
    draw(cx);
    assert!(cx.debug_bounds("overview-empty").is_some());
    assert!(cx.debug_bounds("overview-needs").is_none());
}
