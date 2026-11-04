use super::*;

#[test]
fn a_security_reply_keeps_its_shape_on_the_wire() {
    let reply = Reply::Security {
        repositories: vec![SecurityRepoInfo {
            name: "sheprd-labs/sheprd".into(),
            host: "Local".into(),
            url: "https://github.com/sheprd-labs/sheprd/security/dependabot".into(),
            state: "open".into(),
            total: 1,
            alerts: vec![SecurityAlertInfo {
                number: 2,
                severity: "moderate".into(),
                package: "glib".into(),
                ecosystem: "rust".into(),
                summary: "Unsound iterator".into(),
                advisory: "GHSA-wrw7-89jp-8q8g".into(),
                advisory_url: Some("https://github.com/advisories/GHSA-wrw7-89jp-8q8g".into()),
                vulnerable: ">= 0.15.0, < 0.20.0".into(),
                patched: Some("0.20.0".into()),
                manifest: "Cargo.lock".into(),
                url: "https://github.com/sheprd-labs/sheprd/security/dependabot/2".into(),
            }],
            error: None,
            reporting: "on".into(),
            reports: vec![SecurityReportInfo {
                advisory: "GHSA-aaaa-bbbb-cccc".into(),
                summary: "Path traversal in uploads".into(),
                severity: None,
                reporter: Some("finder".into()),
                created_at: Some("2026-10-01T00:00:00+00:00".into()),
                url: Some(
                    "https://github.com/sheprd-labs/sheprd/security/advisories/GHSA-aaaa-bbbb-cccc"
                        .into(),
                ),
            }],
            more_reports: false,
            reports_error: None,
        }],
        note: None,
    };
    let response = Response::Host { reply };
    let text = serde_json::to_string(&response).unwrap();
    assert!(text.contains(r#""kind":"security""#), "{text}");
    // No error and no note are left out rather than sent as null.
    assert!(!text.contains(r#""error""#), "{text}");
    assert!(!text.contains(r#""note""#), "{text}");
    // Nor is a complete queue's `more_reports`, which older clients lack.
    assert!(!text.contains(r#""more_reports""#), "{text}");
    assert_eq!(serde_json::from_str::<Response>(&text).unwrap(), response);
}
