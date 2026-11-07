//! `security open` shows a listed item's page and refuses anything else.
use super::*;
use crate::control::server::hosts::security::open;
use crate::security::{Alert, Alerts, Item, Repo, Report, Reports, Severity};
use crate::sidebar::ActivityPanel;

fn listed() -> Repo {
    Repo {
        name: "Sheprd-Labs/Sheprd".into(),
        host: "Local".into(),
        alerts: Ok(Alerts::Open {
            alerts: vec![Alert {
                number: 42,
                severity: Severity::Critical,
                package: "rustls-webpki".into(),
                ecosystem: "rust".into(),
                vulnerable: "< 0.102.8".into(),
                patched: Some("0.102.8".into()),
                manifest: "Cargo.lock".into(),
                advisory: "GHSA-xpw8-rcwv-8f8p".into(),
                summary: "Name constraints were ignored".into(),
                created_at: None,
            }],
            total: 1,
        }),
        reports: Ok(Reports::Open {
            reports: vec![Report {
                advisory: "GHSA-aaaa-bbbb-cccc".into(),
                summary: "Path traversal".into(),
                severity: None,
                reporter: None,
                created_at: None,
            }],
            more: false,
        }),
        account: None,
        checkouts: Vec::new(),
    }
}

fn run(
    view: &Entity<SheprdWindow>,
    cx: &mut VisualTestContext,
    repository: &str,
    item: &str,
) -> Response {
    cx.update(|window, cx| view.update(cx, |view, cx| open(view, repository, item, window, cx)))
}

#[gpui::test]
fn an_alert_or_report_the_section_lists_opens_its_page(cx: &mut TestAppContext) {
    let (view, cx) = window(cx);
    cx.update(|_, cx| view.update(cx, |view, _| view.security.read_for_test(vec![listed()])));
    let response = run(&view, cx, "sheprd-labs/sheprd", "#42");
    assert!(matches!(response, Response::Done { .. }), "{response:?}");
    let (panel, opened) = view.read_with(cx, |view, _| {
        (
            view.activity.panel,
            view.security.opened().map(|opened| opened.item.clone()),
        )
    });
    assert_eq!(panel, ActivityPanel::Security);
    assert_eq!(
        opened,
        Some(Item::Alert {
            repo: "Sheprd-Labs/Sheprd".into(),
            number: 42
        })
    );
    let response = run(&view, cx, "Sheprd-Labs/Sheprd", "GHSA-aaaa-bbbb-cccc");
    assert!(matches!(response, Response::Done { .. }), "{response:?}");
}

#[gpui::test]
fn names_the_section_does_not_list_are_refused(cx: &mut TestAppContext) {
    let (view, cx) = window(cx);
    cx.update(|_, cx| view.update(cx, |view, _| view.security.read_for_test(vec![listed()])));
    for (repository, item, code) in [
        ("sheprd-labs/sheprd", "7", ErrorCode::NotFound),
        ("penso/other", "42", ErrorCode::NotFound),
        (
            "sheprd-labs/sheprd",
            "GHSA-zzzz-zzzz-zzzz",
            ErrorCode::NotFound,
        ),
        ("sheprd-labs/sheprd", "latest", ErrorCode::InvalidRequest),
        ("../etc", "42", ErrorCode::InvalidRequest),
    ] {
        let response = run(&view, cx, repository, item);
        assert_eq!(error_code(&response), Some(code), "{repository} {item}");
    }
    let opened = view.read_with(cx, |view, _| view.security.opened().is_some());
    assert!(!opened);
}
