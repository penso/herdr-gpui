use super::*;

#[test]
fn counts_only_critical_and_high_alerts() {
    let mut security = Security::default();
    security.install(
        vec![
            repo(
                "a/one",
                Ok(open(vec![
                    alert(1, Severity::Critical),
                    alert(2, Severity::Moderate),
                    alert(3, Severity::Low),
                ])),
            ),
            repo("a/two", Ok(open(vec![alert(4, Severity::High)]))),
            repo("a/three", Err(Error::Output)),
        ],
        Instant::now(),
        SystemTime::now(),
    );
    assert_eq!(security.urgent(), 2);
}

#[test]
fn reads_again_after_the_interval_or_when_refreshed() {
    let mut security = Security::default();
    let now = Instant::now();
    assert!(security.due(now));
    security.install(Vec::new(), now, SystemTime::now());
    assert!(!security.due(now + Duration::from_secs(60)));
    assert!(security.due(now + INTERVAL));
    security.refresh();
    assert!(security.due(now));
}

#[test]
fn a_spent_quota_is_told_apart_from_other_refusals() {
    let limited = Error::Github(Box::new(
        herdr_github::Error::RateLimit { reset: None }.into(),
    ));
    assert!(limited.rate_limited());
    assert!(!Error::Github(Box::new(herdr_github::Error::Forbidden.into())).rate_limited());
    assert!(!Error::Output.rate_limited());
}

#[test]
fn says_how_long_ago_with_the_shared_relative_time() {
    let now = SystemTime::now();
    let since = |seconds| {
        crate::security::section::since(now, now - Duration::from_secs(seconds))
            .replace(['\u{2068}', '\u{2069}'], "")
    };
    assert_eq!(since(30), "just now");
    assert_eq!(since(5 * 60 + 10), "5 min ago");
    assert_eq!(since(3 * 3600), "3 h ago");
    assert_eq!(since(30 * 3600), "yesterday");
    // A time ahead of the clock reads as now.
    assert_eq!(
        crate::security::section::since(now, now + Duration::from_secs(60)),
        "just now"
    );
}
