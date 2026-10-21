use super::*;
use crate::security::reports::{AdvisoryNode, Reporting, hidden, parse, report_url};

/// Reads `value` as the REST listing is read: straight into the typed nodes.
pub(super) fn parse_json(value: Value) -> Reports {
    parse(serde_json::from_value(value).unwrap())
}

pub(super) fn advisory_json(id: &str, state: &str, severity: Value, created: &str) -> Value {
    advisory(id, state, severity, created)
}

fn advisory(id: &str, state: &str, severity: Value, created: &str) -> Value {
    json!({
        "ghsa_id": id,
        "summary": format!("Report {id}"),
        "description": "Step 1: send this payload…",
        "state": state,
        "severity": severity,
        "author": {"login": "finder"},
        "created_at": created,
        "cvss": {"score": 9.8},
    })
}

#[test]
fn reads_reports_in_triage_rated_first_then_newest() {
    let reports = parse_json(json!([
        advisory(
            "GHSA-aaaa-aaaa-aaaa",
            "triage",
            Value::Null,
            "2026-10-05T00:00:00Z"
        ),
        advisory(
            "GHSA-bbbb-bbbb-bbbb",
            "triage",
            json!("medium"),
            "2026-10-01T00:00:00Z"
        ),
        advisory(
            "GHSA-cccc-cccc-cccc",
            "triage",
            json!("critical"),
            "2026-10-02T00:00:00Z"
        ),
        advisory(
            "GHSA-dddd-dddd-dddd",
            "draft",
            json!("critical"),
            "2026-10-03T00:00:00Z"
        ),
    ]));
    let Reports::Open { reports, more } = reports else {
        panic!("expected open reports");
    };
    let order: Vec<&str> = reports
        .iter()
        .map(|report| report.advisory.as_str())
        .collect();
    // A maintainer's own draft is not a report.
    assert_eq!(
        order,
        [
            "GHSA-cccc-cccc-cccc",
            "GHSA-bbbb-bbbb-bbbb",
            "GHSA-aaaa-aaaa-aaaa"
        ]
    );
    assert_eq!(reports[1].severity, Some(Severity::Moderate));
    assert_eq!(reports[2].severity, None);
    assert_eq!(reports[0].reporter.as_deref(), Some("finder"));
    assert!(!more);
}

#[test]
fn a_report_keeps_no_description() {
    let Reports::Open { reports, .. } = parse_json(json!([advisory(
        "GHSA-aaaa-aaaa-aaaa",
        "triage",
        json!("high"),
        "2026-10-05T00:00:00Z"
    )])) else {
        panic!("expected open reports");
    };
    let held = format!("{reports:?}");
    assert!(!held.contains("payload"), "{held}");
}

#[test]
fn reporting_switch_and_malformed_answers() {
    let enabled = |value| serde_json::from_value::<Reporting>(value).map(|r| r.enabled);
    assert!(enabled(json!({"enabled": true})).unwrap());
    assert!(!enabled(json!({"enabled": false})).unwrap());
    assert!(enabled(json!({})).is_err());
    assert!(serde_json::from_value::<Vec<AdvisoryNode>>(json!({"message": "x"})).is_err());
}

#[test]
fn a_refusal_hides_reports_but_a_spent_quota_is_an_error() {
    assert!(hidden(&crate::Error::GitHubForbidden));
    assert!(hidden(&crate::Error::GitHubStatus(404)));
    assert!(!hidden(&crate::Error::GitHubStatus(500)));
    assert!(!hidden(&crate::Error::GitHubRateLimit { reset: None }));
}

#[test]
fn report_links_need_a_ghsa_identifier() {
    let good = report("GHSA-aaaa-bbbb-cccc", None, "2026-10-05T00:00:00Z");
    assert_eq!(
        report_url("penso/herdr-gpui", &good).as_deref(),
        Some("https://github.com/penso/herdr-gpui/security/advisories/GHSA-aaaa-bbbb-cccc")
    );
    let bad = report("../../x", None, "2026-10-05T00:00:00Z");
    assert_eq!(report_url("penso/herdr-gpui", &bad), None);
}

#[test]
fn every_report_counts_on_the_bar_and_reporting_makes_it_available() {
    let mut security = Security::default();
    security.install(
        vec![reported(
            "a/one",
            vec![
                report("GHSA-aaaa-aaaa-aaaa", None, "2026-10-05T00:00:00Z"),
                report(
                    "GHSA-bbbb-bbbb-bbbb",
                    Some(Severity::Low),
                    "2026-10-05T00:00:00Z",
                ),
            ],
        )],
        Instant::now(),
        SystemTime::now(),
    );
    assert_eq!(security.urgent(), 2);
    let mut quiet = Security::default();
    quiet.install(
        vec![reported("a/two", Vec::new())],
        Instant::now(),
        SystemTime::now(),
    );
    assert_eq!(quiet.urgent(), 0);
}
