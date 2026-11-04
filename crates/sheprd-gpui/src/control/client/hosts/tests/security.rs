use super::*;
use crate::control::protocol::hosts::{SecurityAlertInfo, SecurityRepoInfo};

fn repo(name: &str, state: &str, alerts: Vec<SecurityAlertInfo>) -> SecurityRepoInfo {
    SecurityRepoInfo {
        name: name.into(),
        host: "Local".into(),
        url: format!("https://github.com/{name}/security/dependabot"),
        state: state.into(),
        total: alerts.len() as u64,
        alerts,
        error: (state == "error").then(|| "GitHub refused".into()),
        reporting: "off".into(),
        reports: Vec::new(),
        more_reports: false,
        reports_error: None,
    }
}

#[test]
fn security_reads_as_one_line_per_repository_and_two_per_alert() {
    let alert = SecurityAlertInfo {
        number: 2,
        severity: "critical".into(),
        package: "openssl".into(),
        ecosystem: "rust".into(),
        summary: "Use after free".into(),
        advisory: "GHSA-aaaa-bbbb-cccc".into(),
        advisory_url: None,
        vulnerable: "< 0.10.70".into(),
        patched: Some("0.10.70".into()),
        manifest: "Cargo.lock".into(),
        url: "https://github.com/a/one/security/dependabot/2".into(),
    };
    let reply = Reply::Security {
        repositories: vec![
            repo("a/one", "open", vec![alert]),
            repo("a/quiet", "open", Vec::new()),
            repo("a/off", "off", Vec::new()),
            repo("a/failed", "error", Vec::new()),
        ],
        note: None,
    };
    assert_eq!(
        text::security(&reply),
        "a/one (Local): 1 open\n\
         \x20 critical openssl (rust) Use after free\n\
         \x20          GHSA-aaaa-bbbb-cccc · Cargo.lock · fixed in 0.10.70 · https://github.com/a/one/security/dependabot/2\n\
         a/quiet (Local): no open alerts\n\
         a/off (Local): Dependabot alerts are off\n\
         a/failed (Local): GitHub refused"
    );
}

#[test]
fn reports_read_before_the_alerts_of_their_repository() {
    use crate::control::protocol::hosts::SecurityReportInfo;
    let mut repository = repo("a/one", "open", Vec::new());
    repository.reporting = "on".into();
    repository.reports = vec![SecurityReportInfo {
        advisory: "GHSA-aaaa-bbbb-cccc".into(),
        summary: "Path traversal in uploads".into(),
        severity: None,
        reporter: Some("finder".into()),
        created_at: None,
        url: Some("https://github.com/a/one/security/advisories/GHSA-aaaa-bbbb-cccc".into()),
    }];
    let reply = Reply::Security {
        repositories: vec![repository],
        note: None,
    };
    assert_eq!(
        text::security(&reply),
        "a/one (Local): no open alerts\n\
         \x20 reported Path traversal in uploads [unrated]\n\
         \x20          GHSA-aaaa-bbbb-cccc · by finder · https://github.com/a/one/security/advisories/GHSA-aaaa-bbbb-cccc"
    );
}

#[test]
fn a_queue_cut_short_says_github_holds_more() {
    let mut repository = repo("a/one", "open", Vec::new());
    repository.reporting = "on".into();
    repository.more_reports = true;
    let reply = Reply::Security {
        repositories: vec![repository],
        note: None,
    };
    assert_eq!(
        text::security(&reply),
        "a/one (Local): no open alerts\n\
         \x20 more reports in triage on GitHub"
    );
}

#[test]
fn alerts_a_sign_in_cannot_read_say_so_rather_than_none() {
    let reply = Reply::Security {
        repositories: vec![repo("a/hidden", "hidden", Vec::new())],
        note: None,
    };
    assert_eq!(
        text::security(&reply),
        "a/hidden (Local): Dependabot alerts are not readable with this sign-in"
    );
}
