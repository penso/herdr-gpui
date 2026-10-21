//! The section searches and narrows as Pull requests and Issues do, and
//! every row offers Start agent, in the cards and beside a page.
use super::pane::shown;
use super::section::{draw, install, window};
use super::*;
use crate::security::Item;
use gpui::{Entity, Modifiers, VisualTestContext};

fn shown_items(view: &Entity<HerdrWindow>, cx: &mut VisualTestContext) -> Vec<Item> {
    cx.update(|_, cx| view.read(cx).shown_items())
}

fn click(cx: &mut VisualTestContext, selector: &'static str) {
    let at = cx
        .debug_bounds(selector)
        .unwrap_or_else(|| panic!("{selector}"))
        .center();
    cx.simulate_click(at, Modifiers::default());
    draw(cx);
}

fn type_search(view: &Entity<HerdrWindow>, cx: &mut VisualTestContext, text: &str) {
    cx.update(|window, cx| {
        let focus = view.read(cx).security_search.input.read(cx).focus.clone();
        window.focus(&focus, cx);
    });
    cx.simulate_input(text);
    draw(cx);
}

#[gpui::test]
fn the_search_and_pills_narrow_what_the_section_lists(cx: &mut gpui::TestAppContext) {
    let (view, cx) = window(cx);
    let mut one = repo("a/one", Ok(open(vec![alert(1, Severity::Critical)])));
    one.reports = Ok(Reports::Open {
        reports: vec![report(
            "GHSA-aaaa-bbbb-cccc",
            Some(Severity::High),
            "2026-10-05T00:00:00Z",
        )],
        more: false,
    });
    let mut lodash = alert(7, Severity::Moderate);
    lodash.package = "lodash".into();
    let two = repo("a/two", Ok(open(vec![lodash])));
    install(&view, cx, vec![one, two]);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.show_activity_panel(ActivityPanel::Security, window, cx);
        });
    });
    draw(cx);
    assert_eq!(shown_items(&view, cx).len(), 3);
    for pill in [
        "security-pill-reports",
        "security-pill-dependabot",
        "security-pill-repo-0",
        "security-pill-repo-1",
    ] {
        assert!(cx.debug_bounds(pill).is_some(), "{pill}");
    }

    // The field keeps what names the package; a source with nothing left
    // drops its pill.
    type_search(&view, cx, "LODASH");
    assert_eq!(
        shown_items(&view, cx),
        [Item::Alert {
            repo: "a/two".into(),
            number: 7
        }]
    );
    assert!(cx.debug_bounds("security-pill-reports").is_none());
    click(cx, "security-search-clear");
    assert_eq!(shown_items(&view, cx).len(), 3);

    // A source pill keeps its items; a repository without them leaves none.
    click(cx, "security-pill-reports");
    assert_eq!(
        shown_items(&view, cx),
        [Item::Report {
            repo: "a/one".into(),
            advisory: "GHSA-aaaa-bbbb-cccc".into()
        }]
    );
    click(cx, "security-pill-repo-1");
    assert!(shown_items(&view, cx).is_empty());
    assert!(cx.debug_bounds("security-section-no-match").is_some());
}

#[gpui::test]
fn every_row_offers_start_agent_and_stepping_keeps_to_the_search(cx: &mut gpui::TestAppContext) {
    let (view, cx) = shown(cx);
    let opened = |cx: &mut VisualTestContext| {
        cx.update(|_, cx| {
            view.read(cx)
                .security
                .opened()
                .map(|open| open.item.clone())
        })
    };
    // A card's button (the last drawn: the last alert's) starts an agent
    // rather than opening the row's page.
    let row = cx.debug_bounds("security-alert").unwrap();
    let start = cx.debug_bounds("security-start").unwrap();
    assert!(row.contains(&start.center()), "{start:?} in {row:?}");
    click(cx, "security-start");
    assert_eq!(opened(cx), None);

    // Searching for "critical" keeps only the first alert; beside its page
    // the list still offers Start agent, and Next has nowhere to go.
    type_search(&view, cx, "critical");
    click(cx, "security-alert");
    let first = Item::Alert {
        repo: "a/one".into(),
        number: 1,
    };
    assert_eq!(opened(cx), Some(first.clone()));
    let list = cx.debug_bounds("security-list").unwrap();
    let start = cx.debug_bounds("security-start").unwrap();
    assert!(list.contains(&start.center()), "{start:?} in {list:?}");
    cx.update(|_, cx| view.update(cx, |view, cx| view.step_security_item(1, cx)));
    draw(cx);
    assert_eq!(opened(cx), Some(first));
}

#[gpui::test]
fn a_picked_pill_stays_to_be_unpicked_when_it_is_the_only_one_left(cx: &mut gpui::TestAppContext) {
    let (view, cx) = window(cx);
    let mut lodash = alert(7, Severity::Moderate);
    lodash.package = "lodash".into();
    install(
        &view,
        cx,
        vec![
            repo("a/one", Ok(open(vec![alert(1, Severity::Critical)]))),
            repo("a/two", Ok(open(vec![lodash]))),
        ],
    );
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.show_activity_panel(ActivityPanel::Security, window, cx);
        });
    });
    draw(cx);
    click(cx, "security-pill-repo-1");
    // Only the picked repository holds a match: its pill is the group's
    // last, and still there to unpick.
    type_search(&view, cx, "lodash");
    assert_eq!(shown_items(&view, cx).len(), 1);
    assert!(cx.debug_bounds("security-pill-repo-1").is_none());
    click(cx, "security-pill-repo-0");
    click(cx, "security-search-clear");
    assert!(!cx.update(|_, cx| view.read(cx).security_search.narrows()));
    assert_eq!(shown_items(&view, cx).len(), 2);
}
