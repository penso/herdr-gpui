use super::*;

#[test]
fn unified_billing_shows_wallet_and_period_without_inventing_usage() {
    let body = r#"{"config":{
        "currentPeriod":{"type":"USAGE_PERIOD_TYPE_WEEKLY",
            "start":"2026-08-06T00:00:00Z","end":"2026-08-13T00:00:00Z"},
        "isUnifiedBillingUser":true,"prepaidBalance":{"val":1446},
        "topUpMethod":"TOP_UP_METHOD_MANUAL",
        "billingPeriodStart":"2026-08-06T00:00:00Z",
        "billingPeriodEnd":"2026-08-13T00:00:00Z"}}"#;
    let report = parse(
        body,
        Some("SuperGrok"),
        Account::default(),
        at("2026-08-12T00:00:00Z"),
    )
    .unwrap();
    assert!(report.windows.is_empty());
    assert_eq!(report.account.plan.as_deref(), Some("SuperGrok"));
    assert_eq!(
        report.balances,
        vec![Balance::new(
            "Prepaid balance",
            14.46,
            Unit::Currency("USD".into())
        )]
    );
    assert_eq!(
        report.sections,
        vec![Section::Facts {
            title: "Included credits".into(),
            facts: vec![
                ("Usage".into(), "Not reported".into()),
                ("Period ends".into(), "Aug 13, 2026 00:00 UTC".into()),
            ],
        }]
    );
}

#[test]
fn wallet_alone_is_a_valid_report_including_zero_and_string_cents() {
    for (wallet, amount) in [
        (r#"{"val":1446}"#, 14.46),
        ("{}", 0.),
        (r#"{"val":0}"#, 0.),
        (r#"{"val":"1446"}"#, 14.46),
    ] {
        let body = format!(r#"{{"config":{{"prepaidBalance":{wallet}}}}}"#);
        let report = parse(&body, None, Account::default(), SystemTime::UNIX_EPOCH).unwrap();
        assert!(report.windows.is_empty());
        assert_eq!(
            report.balances,
            vec![Balance::new(
                "Prepaid balance",
                amount,
                Unit::Currency("USD".into())
            )]
        );
        assert_eq!(
            report.sections,
            vec![Section::Facts {
                title: "Included credits".into(),
                facts: vec![("Usage".into(), "Not reported".into())],
            }]
        );
    }
}

#[test]
fn wallet_does_not_replace_included_quota() {
    // Older servers omit the migration flag; both migrated and unmigrated
    // accounts can still report a quota, independently of their wallet.
    for migration in [
        "",
        r#", "isUnifiedBillingUser":false"#,
        r#", "isUnifiedBillingUser":true"#,
    ] {
        for wallet in ["", r#", "prepaidBalance":{"val":1446}"#] {
            let body = format!(r#"{{"config":{{"creditUsagePercent":12.5{migration}{wallet}}}}}"#);
            let report = parse(&body, None, Account::default(), SystemTime::UNIX_EPOCH).unwrap();
            assert_eq!(report.windows[0].used, 12.5);
            assert!(report.sections.is_empty());
            if wallet.is_empty() {
                assert!(report.balances.is_empty());
            } else {
                assert_eq!(report.balances[0].amount, 14.46);
            }
        }
    }
}

#[test]
fn invalid_wallet_is_absent_and_does_not_discard_valid_quota() {
    for wallet in [
        "null",
        "false",
        "[]",
        "42",
        r#""1446""#,
        r#"{"val":null}"#,
        r#"{"val":-1}"#,
        r#"{"val":"-1"}"#,
        r#"{"val":1.5}"#,
        r#"{"val":"1.5"}"#,
        r#"{"val":"invalid"}"#,
        r#"{"val":true}"#,
        r#"{"val":9223372036854775808}"#,
        r#"{"unknown":0}"#,
    ] {
        let body = format!(r#"{{"config":{{"creditUsagePercent":25,"prepaidBalance":{wallet}}}}}"#);
        let report = parse(&body, None, Account::default(), SystemTime::UNIX_EPOCH).unwrap();
        assert_eq!(report.windows[0].used, 25., "{wallet}");
        assert!(report.balances.is_empty(), "{wallet}");
        let body = format!(r#"{{"config":{{"prepaidBalance":{wallet}}}}}"#);
        assert!(
            matches!(
                parse(&body, None, Account::default(), SystemTime::UNIX_EPOCH),
                Err(crate::Error::Usage(Error::Json(
                    serde_json::error::Category::Data
                )))
            ),
            "{wallet}"
        );
    }
}
