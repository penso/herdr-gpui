//! The `en-XA` leak test for the Security section: its cards and notes, the
//! list beside an open item, and every tab of an alert's and a report's
//! page.
use super::section::{draw, install, window};
use super::*;
use crate::{
    i18n::leak_tests::{assert_translated, uncut, under_en_xa},
    security::{
        Item,
        page::{
            Affected, AlertPage, Comment, Comments, Credit, Cvss, Loaded, Page, Relationship,
            ReportPage, Scope, Weakness,
        },
    },
};
use gpui::{Entity, VisualTestContext, px, size};

const ADVISORY: &str = "GHSA-aaaa-bbbb-cccc";

fn weakness() -> Weakness {
    Weakness {
        id: "CWE-295".into(),
        name: "Improper Certificate Validation".into(),
    }
}

fn cvss() -> Cvss {
    Cvss {
        version: "3.1",
        score: 7.5,
        vector: "AV:N/AC:L/PR:N/UI:N/S:U/C:H/I:N/A:N".into(),
    }
}

fn alert_page() -> Page {
    Page::Alert(AlertPage {
        description: crate::release_notes::parse("Name constraints were ignored."),
        published_at: Some("2026-10-01T00:00:00Z".parse().unwrap()),
        cvss: Some(cvss()),
        weaknesses: vec![weakness()],
        identifiers: vec!["CVE-2024-12224".into()],
        references: vec!["https://nvd.nist.gov/vuln/detail/CVE-2024-12224".into()],
        requirement: Some("= 0.19.0".into()),
        relationship: Relationship::Transitive,
        scope: Scope::Runtime,
        update: Some(42),
    })
}

fn report_page() -> Page {
    Page::Report(ReportPage {
        description: crate::release_notes::parse("A page can read any file."),
        affected: vec![Affected {
            package: "glib".into(),
            ecosystem: "rust".into(),
            vulnerable: "< 0.20.0".into(),
            patched: "0.20.0".into(),
        }],
        cvss: Some(cvss()),
        weaknesses: vec![weakness()],
        credits: vec![Credit {
            login: "finder".into(),
            kind: "reporter".into(),
        }],
        comments: Comments::Read {
            comments: vec![Comment {
                author: Some("maintainer".into()),
                body: crate::release_notes::parse("Thanks, looking."),
                created_at: Some("2026-10-06T00:00:00Z".parse().unwrap()),
            }],
            more: true,
        },
    })
}

/// Repositories with every kind of line the section draws.
fn repos() -> Vec<Repo> {
    let mut one = repo(
        "a/one",
        Ok(open(vec![
            alert(1, Severity::Critical),
            alert(2, Severity::Low),
        ])),
    );
    one.reports = Ok(Reports::Open {
        reports: vec![report(
            ADVISORY,
            Some(Severity::High),
            "2026-10-05T00:00:00Z",
        )],
        more: false,
    });
    vec![
        one,
        repo("a/quiet", Ok(open(Vec::new()))),
        repo("a/off", Ok(Alerts::Off)),
        repo("a/hidden", Ok(Alerts::Hidden)),
        repo("a/failed", Err(Error::Output)),
    ]
}

/// What GitHub and the repositories hold: names, packages, advisories,
/// summaries, versions, links, and the upgrade command. Errors are
/// messages of their own, so none is passed.
fn data() -> Vec<String> {
    let alert = alert(1, Severity::Critical);
    let mut data: Vec<String> = [
        "a/one",
        "a/quiet",
        "a/off",
        "a/hidden",
        "a/failed",
        "Local",
        ADVISORY,
        "Report GHSA-aaaa-bbbb-cccc",
        "finder",
        "maintainer",
        "Name constraints were ignored.",
        "A page can read any file.",
        "Thanks, looking.",
        "CVE-2024-12224",
        "https://nvd.nist.gov/vuln/detail/CVE-2024-12224",
        "https://github.com/a/one/pull/42",
        "AV:N/AC:L/PR:N/UI:N/S:U/C:H/I:N/A:N",
        "CVSS 3.1 AV:N/AC:L/PR:N/UI:N/S:U/C:H/I:N/A:N",
        "CWE-295 Improper Certificate Validation",
        "finder (reporter)",
        "glib (rust)",
        "glib < 0.20.0",
        "cargo update -p glib --precise 0.20.0",
        "= 0.19.0",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect();
    data.extend([
        alert.package,
        alert.ecosystem,
        alert.summary.clone(),
        format!("{} — {}", "glib", alert.summary),
        alert.vulnerable,
        alert.manifest,
        alert.advisory,
    ]);
    data
}

/// A row's detail line joins its facts with " · ", each a message or data
/// of its own, so a line that is not one message is checked fact by fact.
fn facts(shown: Vec<String>) -> Vec<String> {
    uncut(shown)
        .into_iter()
        .flat_map(|line| {
            if herdr_i18n::pseudo::is_marked(&line) {
                vec![line]
            } else {
                line.split(" · ").map(str::to_owned).collect()
            }
        })
        .collect()
}

fn open_item(view: &Entity<HerdrWindow>, cx: &mut VisualTestContext, item: Item, page: Page) {
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

#[test]
fn security_shows_only_messages_and_what_github_holds() {
    under_en_xa("Security en-XA leak", |cx, recorder| {
        let (view, cx) = window(cx);
        cx.simulate_resize(size(px(2400.), px(1600.)));
        // The window around the section is other modules' text.
        draw(cx);
        recorder.take();
        let data = data();

        install(&view, cx, repos());
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                view.show_activity_panel(ActivityPanel::Security, window, cx);
            });
        });
        draw(cx);
        assert_translated("Security section", &facts(recorder.take()), &data);

        for (item, page) in [
            (
                Item::Alert {
                    repo: "a/one".into(),
                    number: 1,
                },
                alert_page(),
            ),
            (
                Item::Report {
                    repo: "a/one".into(),
                    advisory: ADVISORY.into(),
                },
                report_page(),
            ),
        ] {
            open_item(&view, cx, item.clone(), page);
            for &tab in item.tabs() {
                cx.update(|_, cx| view.update(cx, |view, cx| view.show_security_tab(tab, cx)));
                draw(cx);
                assert_translated(
                    &format!("{item:?} on {tab:?}"),
                    &facts(recorder.take()),
                    &data,
                );
            }
        }
    });
}
