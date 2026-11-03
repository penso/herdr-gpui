use super::*;
use crate::security::model::{parse, valid_name};

#[test]
fn reads_open_alerts_most_severe_then_newest_first() {
    let alerts = parse(&response(
        true,
        5,
        vec![
            node(1, "LOW", "2026-09-01T00:00:00Z", "rand"),
            node(2, "CRITICAL", "2026-09-02T00:00:00Z", "openssl"),
            node(3, "MODERATE", "2026-09-03T00:00:00Z", "glib"),
            node(4, "CRITICAL", "2026-09-04T00:00:00Z", "ring"),
        ],
    ))
    .unwrap();
    let Alerts::Open { alerts, total } = alerts else {
        panic!("expected open alerts");
    };
    assert_eq!(total, 5);
    let order: Vec<u64> = alerts.iter().map(|alert| alert.number).collect();
    assert_eq!(order, [4, 2, 3, 1]);
    let first = &alerts[0];
    assert_eq!(first.severity, Severity::Critical);
    assert_eq!(first.package, "ring");
    assert_eq!(first.ecosystem, "rust");
    assert_eq!(first.patched.as_deref(), Some("0.20.0"));
    assert_eq!(first.manifest, "Cargo.lock");
    assert_eq!(
        first.advisory_url().as_deref(),
        Some("https://github.com/advisories/GHSA-wrw7-89jp-8q8g")
    );
}

#[test]
fn alerts_turned_off_read_as_off_whatever_the_listing_says() {
    assert_eq!(parse(&response(false, 0, Vec::new())).unwrap(), Alerts::Off);
}

#[test]
fn a_hidden_repository_is_an_error_not_an_empty_listing() {
    let error = parse(&json!({"data": {"repository": null}})).unwrap_err();
    assert!(matches!(error, Error::Output));
    assert!(matches!(parse(&json!({})).unwrap_err(), Error::Output));
}

#[test]
fn an_unknown_severity_fails_rather_than_passing_for_a_lesser_one() {
    let error = parse(&response(
        true,
        1,
        vec![node(1, "SEVERE", "2026-09-01T00:00:00Z", "rand")],
    ))
    .unwrap_err();
    assert!(matches!(error, Error::Json(_)));
}

#[test]
fn an_alert_without_its_vulnerability_is_left_out_but_counted() {
    let mut withdrawn = node(1, "LOW", "2026-09-01T00:00:00Z", "rand");
    withdrawn["securityVulnerability"] = Value::Null;
    let Alerts::Open { alerts, total } = parse(&response(true, 1, vec![withdrawn])).unwrap() else {
        panic!("expected open alerts");
    };
    assert!(alerts.is_empty());
    assert_eq!(total, 1);
}

#[test]
fn remote_text_is_cleaned_and_bounded() {
    let mut hostile = node(1, "HIGH", "2026-09-01T00:00:00Z", "evil\u{1b}[31mpkg");
    hostile["securityVulnerability"]["advisory"]["summary"] = json!("x".repeat(5_000));
    hostile["securityVulnerability"]["advisory"]["ghsaId"] = json!("../../phish");
    let Alerts::Open { alerts, .. } = parse(&response(true, 1, vec![hostile])).unwrap() else {
        panic!("expected open alerts");
    };
    assert!(!alerts[0].package.chars().any(char::is_control));
    assert!(alerts[0].summary.chars().count() <= 512);
    // An identifier without GHSA's shape links nowhere.
    assert_eq!(alerts[0].advisory_url(), None);
}

#[test]
fn only_plain_owner_and_name_make_an_address() {
    assert!(valid_name("sheprd-labs/sheprd"));
    assert!(valid_name("rust-lang/rust.vim"));
    for bad in [
        "penso",
        "penso/",
        "/herdr",
        "a/b/c",
        "../x",
        "a/..",
        "penso/herdr gpui",
        "penso/herdr?x=1",
    ] {
        assert!(!valid_name(bad), "{bad}");
    }
}

#[test]
fn alert_links_point_into_the_repository_security_tab() {
    let repo = repo(
        "sheprd-labs/sheprd",
        Ok(open(vec![alert(7, Severity::High)])),
    );
    assert_eq!(
        repo.alert_url(&repo.open()[0]),
        "https://github.com/sheprd-labs/sheprd/security/dependabot/7"
    );
    assert_eq!(
        repo.alerts_url(),
        "https://github.com/sheprd-labs/sheprd/security/dependabot"
    );
}
