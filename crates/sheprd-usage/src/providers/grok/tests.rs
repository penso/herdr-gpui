#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;
use crate::text::Text;

fn at(text: &str) -> SystemTime {
    Timestamp::Text(text.into()).time().unwrap()
}

#[test]
fn parses_weekly_credits() {
    let body = r#"{
      "config": {
        "creditUsagePercent": 12.5,
        "currentPeriod": {
          "type": "USAGE_PERIOD_TYPE_WEEKLY",
          "start": "2026-08-06T00:00:00Z",
          "end": "2026-08-13T00:00:00Z"
        },
        "billingPeriodEnd": "2026-08-13T00:00:00Z",
        "onDemandCap": { "val": 1000 },
        "onDemandUsed": { "val": 250 }
      }
    }"#;
    let account = Account {
        email: Some(Text::service("user@example.com")),
        plan: login_plan("oidc").map(Text::service),
    };
    let report = parse(
        body,
        Some("SuperGrok Heavy"),
        account,
        at("2026-08-12T00:00:00Z"),
    )
    .unwrap();
    assert_eq!(report.windows.len(), 1);
    assert_eq!(report.windows[0].kind, Kind::Weekly);
    assert_eq!(report.windows[0].used, 12.5);
    assert_eq!(report.windows[0].length, Some(WEEK));
    assert_eq!(
        report.windows[0].resets_at,
        Some(at("2026-08-13T00:00:00Z"))
    );
    assert_eq!(
        report.account.plan.as_ref().map(Text::as_str),
        Some("SuperGrok Heavy")
    );
    assert_eq!(
        report.account.email.as_ref().map(Text::as_str),
        Some("user@example.com")
    );
}

#[test]
fn falls_back_to_on_demand_ratio_and_billing_period() {
    let body = r#"{"config":{
        "billingPeriodStart":"2026-07-13T00:00:00Z",
        "billingPeriodEnd":"2026-08-13T00:00:00Z",
        "onDemandCap":{"val":1000},"onDemandUsed":{"val":250}}}"#;
    let report = parse(body, None, Account::default(), at("2026-08-12T00:00:00Z")).unwrap();
    assert_eq!(report.windows[0].percent(), 25);
    assert_eq!(report.windows[0].kind, Kind::Monthly);
}

#[test]
fn period_without_percent_is_unknown() {
    let body = r#"{"config":{"currentPeriod":{"start":"2026-08-06T00:00:00Z",
        "end":"2026-08-13T00:00:00Z"}}}"#;
    let report = parse(body, None, Account::default(), at("2026-08-12T00:00:00Z")).unwrap();
    assert!(report.windows.is_empty());
    assert_eq!(report.sections.len(), 1);
}

#[test]
fn future_start_leaves_length_unknown() {
    let body = r#"{"config":{"creditUsagePercent":90,
        "currentPeriod":{"start":"2026-08-14T00:00:00Z","end":"2026-08-21T00:00:00Z"}}}"#;
    let report = parse(body, None, Account::default(), at("2026-08-12T00:00:00Z")).unwrap();
    assert_eq!(report.windows[0].length, None);
    assert_eq!(
        report.windows[0].kind,
        Kind::Named(Text::message("usage-grok-credits"))
    );
}

#[test]
fn empty_answer_fails() {
    assert!(parse("{}", None, Account::default(), SystemTime::now()).is_err());
    assert!(
        parse(
            r#"{"config":{}}"#,
            None,
            Account::default(),
            SystemTime::now()
        )
        .is_err()
    );
}

#[test]
fn names_plans() {
    assert_eq!(
        plan_name("super_grok_heavy").as_deref(),
        Some("SuperGrok Heavy")
    );
    assert_eq!(plan_name("SuperGrok").as_deref(), Some("SuperGrok"));
    assert_eq!(login_plan("session").as_deref(), Some("session"));
}

mod prepaid;
