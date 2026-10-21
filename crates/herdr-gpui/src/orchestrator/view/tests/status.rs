//! An item's page leads with a status box, under a header that steps
//! through the list.
use super::*;
use crate::orchestrator::view::rows::Tab;
use gpui::{Modifiers, TestAppContext};
use std::sync::Arc;

#[gpui::test]
fn an_issue_leads_with_its_status_and_steps_to_the_next(cx: &mut TestAppContext) {
    let (_dir, view, cx) = empty::open(cx);
    let first = item(Provider::Github, "377", "Icons", "2026-10-04T00:00:00Z");
    let second = item(Provider::Github, "378", "Menus", "2026-10-03T00:00:00Z");
    let (first_key, second_key) = (first.key.canonical(), second.key.canonical());
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.repo_mut().snapshot.items = Arc::new(vec![first, second]);
            view.tab = Tab::Issues;
            view.refresh_rows();
            view.open_detail(first_key.clone(), cx);
        })
    });
    cx.run_until_parked();
    let status = cx.debug_bounds("orchestrator-status").unwrap();
    let title = cx.debug_bounds("orchestrator-detail-title").unwrap();
    assert!(status.top() > title.bottom());
    let next = cx.debug_bounds("orchestrator-detail-next").unwrap();
    cx.simulate_click(next.center(), Modifiers::none());
    cx.run_until_parked();
    cx.update(|_, cx| {
        let view = view.read(cx);
        assert_eq!(
            view.detail.as_ref().map(|d| d.key.as_str()),
            Some(second_key.as_str())
        );
        assert_eq!(view.selected.as_deref(), Some(second_key.as_str()));
    });
    // Past the end it stays put.
    cx.update(|_, cx| view.update(cx, |view, cx| view.step_detail(1, cx)));
    cx.update(|_, cx| {
        let view = view.read(cx);
        assert_eq!(
            view.detail.as_ref().map(|d| d.key.as_str()),
            Some(second_key.as_str())
        );
    });
    let close = cx.debug_bounds("orchestrator-detail-close").unwrap();
    cx.simulate_click(close.center(), Modifiers::none());
    cx.run_until_parked();
    assert!(cx.debug_bounds("orchestrator-status").is_none());
}

#[gpui::test]
fn a_pull_request_says_which_checks_fail_and_what_blocks_the_merge(cx: &mut TestAppContext) {
    let (_dir, view, cx) = empty::open(cx);
    let pr = crate::pull_request::fixture().unwrap();
    cx.update(|_, cx| {
        let view = view.read(cx);
        let rows = view.pull_request_rows(&pr);
        let titles: Vec<&str> = rows.iter().map(|row| row.title.as_ref()).collect();
        assert_eq!(
            titles,
            ["1 of 3 checks failing", "Review required", "Merge blocked"]
        );
        assert_eq!(rows[0].detail.as_deref(), Some("ci/lint"));
        assert_eq!(rows[2].detail.as_deref(), Some("Open into main"));
    });
}
