//! An item opens beside the list, as an issue does, instead of in a browser
//! tab; the list folds away for a wide page, and closing returns the cards.
use super::section::{draw, install, window};
use super::*;
use crate::security::{
    Item, Tab,
    page::{AlertPage, Comments, Loaded, Page, Relationship, ReportPage, Scope},
};
use gpui::{Entity, Modifiers, VisualTestContext};

fn alert_item() -> Item {
    Item::Alert {
        repo: "a/one".into(),
        number: 1,
    }
}

fn report_item() -> Item {
    Item::Report {
        repo: "a/one".into(),
        advisory: "GHSA-aaaa-bbbb-cccc".into(),
    }
}

pub(super) fn shown(
    cx: &mut gpui::TestAppContext,
) -> (Entity<HerdrWindow>, &mut VisualTestContext) {
    let (view, cx) = window(cx);
    let mut one = repo(
        "a/one",
        Ok(open(vec![
            alert(1, Severity::Critical),
            alert(2, Severity::Low),
        ])),
    );
    one.reports = Ok(Reports::Open {
        reports: vec![report(
            "GHSA-aaaa-bbbb-cccc",
            Some(Severity::High),
            "2026-10-05T00:00:00Z",
        )],
        more: false,
    });
    install(&view, cx, vec![one]);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.show_activity_panel(ActivityPanel::Security, window, cx);
        });
    });
    draw(cx);
    (view, cx)
}

fn read(view: &Entity<HerdrWindow>, cx: &mut VisualTestContext, item: Item, page: Page) {
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.security
                .pages
                .insert(item.clone(), Loaded::Read(Arc::new(page)));
            view.open_security_item(item, cx);
        });
    });
    draw(cx);
}

fn alert_page() -> Page {
    Page::Alert(AlertPage {
        description: crate::release_notes::parse("Name constraints were ignored."),
        published_at: None,
        cvss: None,
        weaknesses: Vec::new(),
        identifiers: Vec::new(),
        references: vec!["https://nvd.nist.gov/vuln/detail/CVE-2024-12224".into()],
        requirement: Some("= 0.19.0".into()),
        relationship: Relationship::Transitive,
        scope: Scope::Runtime,
        update: None,
    })
}

#[gpui::test]
fn clicking_an_alert_opens_its_page_beside_the_list(cx: &mut gpui::TestAppContext) {
    let (view, cx) = shown(cx);
    let row = cx.debug_bounds("security-alert").unwrap();
    cx.simulate_click(row.center(), Modifiers::default());
    draw(cx);
    // Selectors find the last match: the second alert's row.
    let opened = view.read_with(cx, |view, _| view.security.opened().cloned());
    assert_eq!(
        opened.map(|opened| opened.item),
        Some(Item::Alert {
            repo: "a/one".into(),
            number: 2
        })
    );
    assert!(cx.debug_bounds("security-list").is_some());
    assert!(cx.debug_bounds("security-row-selected").is_some());
    assert!(cx.debug_bounds("security-page").is_some());
    assert!(cx.debug_bounds("security-callout").is_some());
    assert!(cx.debug_bounds("security-repo-0").is_none());
    // The fixture has no GitHub account, so its page cannot be read; the
    // listing's facts still show, with a way to try again.
    assert!(cx.debug_bounds("security-page-failed").is_some());
}

#[gpui::test]
fn a_read_page_shows_its_facts_and_the_list_folds_into_a_column(cx: &mut gpui::TestAppContext) {
    let (view, cx) = shown(cx);
    read(&view, cx, alert_item(), alert_page());
    assert!(cx.debug_bounds("security-facts").is_some());
    assert!(
        cx.debug_bounds("security-alert-description-line-0")
            .is_some()
    );
    assert!(cx.debug_bounds("security-upgrade-command").is_some());
    let list = cx.debug_bounds("security-page-list").unwrap();
    cx.simulate_click(list.center(), Modifiers::default());
    draw(cx);
    assert!(cx.debug_bounds("security-list").is_none());
    assert!(cx.debug_bounds("security-facts-column").is_some());
    let page = cx.debug_bounds("security-page").unwrap();
    let section = cx.debug_bounds("security-section").unwrap();
    assert_eq!(page.size.width, section.size.width);
}

#[gpui::test]
fn tabs_switch_and_closing_returns_the_cards(cx: &mut gpui::TestAppContext) {
    let (view, cx) = shown(cx);
    read(&view, cx, alert_item(), alert_page());
    let tab = cx.debug_bounds("security-tab-Advisory").unwrap();
    cx.simulate_click(tab.center(), Modifiers::default());
    draw(cx);
    assert!(cx.debug_bounds("security-references").is_some());
    let tab = view.read_with(cx, |view, _| {
        view.security.opened().map(|opened| opened.tab)
    });
    assert_eq!(tab, Some(Tab::Advisory));
    let close = cx.debug_bounds("security-page-close").unwrap();
    cx.simulate_click(close.center(), Modifiers::default());
    draw(cx);
    assert!(cx.debug_bounds("security-page").is_none());
    assert!(cx.debug_bounds("security-repo-0").is_some());
}

#[gpui::test]
fn stepping_walks_the_lists_order(cx: &mut gpui::TestAppContext) {
    let (view, cx) = shown(cx);
    read(
        &view,
        cx,
        report_item(),
        Page::Report(report_page(Comments::None)),
    );
    let opened = |cx: &mut VisualTestContext| {
        view.read_with(cx, |view, _| {
            view.security.opened().map(|opened| opened.item.clone())
        })
    };
    cx.update(|_, cx| view.update(cx, |view, cx| view.step_security_item(-1, cx)));
    assert_eq!(opened(cx), Some(report_item()));
    cx.update(|_, cx| view.update(cx, |view, cx| view.step_security_item(1, cx)));
    assert_eq!(opened(cx), Some(alert_item()));
}

fn report_page(comments: Comments) -> ReportPage {
    ReportPage {
        description: crate::release_notes::parse("A page can read any file."),
        affected: Vec::new(),
        cvss: None,
        weaknesses: Vec::new(),
        credits: Vec::new(),
        comments,
    }
}

#[gpui::test]
fn a_reports_page_says_it_is_private_and_why_comments_are_missing(cx: &mut gpui::TestAppContext) {
    let (view, cx) = shown(cx);
    read(
        &view,
        cx,
        report_item(),
        Page::Report(report_page(Comments::Unavailable)),
    );
    assert!(cx.debug_bounds("security-callout").is_some());
    assert!(cx.debug_bounds("security-report-agent").is_some());
    assert!(
        cx.debug_bounds("security-report-description-line-0")
            .is_some()
    );
    cx.update(|_, cx| {
        view.update(cx, |view, cx| view.show_security_tab(Tab::Conversation, cx));
    });
    draw(cx);
    assert!(
        cx.debug_bounds("security-conversation-unavailable")
            .is_some()
    );
    // An alert's tab is not one of a report's.
    cx.update(|_, cx| view.update(cx, |view, cx| view.show_security_tab(Tab::Advisory, cx)));
    let tab = view.read_with(cx, |view, _| {
        view.security.opened().map(|opened| opened.tab)
    });
    assert_eq!(tab, Some(Tab::Conversation));
}
