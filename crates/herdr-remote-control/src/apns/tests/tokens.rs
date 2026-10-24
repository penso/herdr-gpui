use super::*;
use crate::apns::tokens::MAX_ACTIVITIES;

#[test]
fn tokens_are_bounded_lowercase_hex() {
    let token = DeviceToken::parse(&"AB".repeat(32)).unwrap();
    assert_eq!(token.as_str(), "ab".repeat(32));
    for bad in [
        "",
        "abc",
        &"ab".repeat(8 - 1),
        &"ab".repeat(257),
        &"zz".repeat(32),
        &format!("{}/../x", "ab".repeat(32)),
    ] {
        assert!(
            matches!(DeviceToken::parse(bad), Err(Error::DeviceToken)),
            "{bad}"
        );
    }
}

#[test]
fn parses_the_api_request() {
    let body = format!(
        r#"{{"push_to_start":"{}","activities":[{{"pane":"w1-p3","token":"{}"}}]}}"#,
        "aa".repeat(40),
        "bb".repeat(80)
    );
    let registration = Registration::parse(body.as_bytes()).unwrap();
    assert_eq!(
        registration.push_to_start.as_ref().map(DeviceToken::as_str),
        Some("aa".repeat(40).as_str())
    );
    assert_eq!(
        registration.activity("w1-p3").map(DeviceToken::as_str),
        Some("bb".repeat(80).as_str())
    );
    assert!(Registration::parse(b"{}").unwrap().is_empty());
}

#[test]
fn refuses_bad_requests() {
    let error = Registration::parse(br#"{"push_to_start":"nothex"}"#).unwrap_err();
    assert!(matches!(error, Error::Json(_)), "{error:?}");
    let error = Registration::parse(br#"{"extra":1}"#).unwrap_err();
    assert!(matches!(error, Error::Json(_)), "{error:?}");

    let activity = |pane: &str| format!(r#"{{"pane":"{pane}","token":"{}"}}"#, "cc".repeat(32));
    let many = vec![activity("p"); MAX_ACTIVITIES + 1].join(",");
    let error =
        Registration::parse(format!(r#"{{"activities":[{many}]}}"#).as_bytes()).unwrap_err();
    assert!(
        matches!(error, Error::TooManyActivities { .. }),
        "{error:?}"
    );

    let error = Registration::parse(format!(r#"{{"activities":[{}]}}"#, activity("")).as_bytes())
        .unwrap_err();
    assert!(matches!(error, Error::Pane), "{error:?}");
}

#[test]
fn merge_keeps_push_to_start_and_replaces_activities() {
    let mut stored = Registration {
        activities: vec![ActivityToken {
            pane: "old".to_owned(),
            token: token(1),
        }],
        push_to_start: Some(token(2)),
    };
    stored.merge(Registration {
        activities: vec![
            ActivityToken {
                pane: "a".to_owned(),
                token: token(3),
            },
            ActivityToken {
                pane: "a".to_owned(),
                token: token(4),
            },
        ],
        push_to_start: None,
    });
    assert_eq!(stored.push_to_start, Some(token(2)));
    assert_eq!(stored.activity("old"), None);
    assert_eq!(
        stored.activity("a"),
        Some(&token(4)),
        "the later token wins"
    );
    assert_eq!(stored.activities.len(), 1);

    stored.merge(Registration {
        activities: Vec::new(),
        push_to_start: Some(token(5)),
    });
    assert_eq!(stored.push_to_start, Some(token(5)));
}

#[test]
fn forgets_invalid_tokens_and_ended_activities() {
    let mut stored = Registration {
        activities: vec![
            ActivityToken {
                pane: "a".to_owned(),
                token: token(1),
            },
            ActivityToken {
                pane: "b".to_owned(),
                token: token(2),
            },
        ],
        push_to_start: Some(token(3)),
    };
    assert!(stored.forget(&token(1)));
    assert!(!stored.forget(&token(9)));
    assert!(stored.forget(&token(3)));
    assert_eq!(stored.push_to_start, None);
    assert!(stored.end("b"));
    assert!(stored.is_empty());
    assert_eq!(stored.to_stored().unwrap(), None);
}

#[test]
fn round_trips_through_the_device_store_slot() {
    let stored = Registration {
        activities: vec![ActivityToken {
            pane: "a".to_owned(),
            token: token(1),
        }],
        push_to_start: Some(token(2)),
    };
    let value = stored.to_stored().unwrap();
    assert_eq!(Registration::from_stored(value.as_ref()), stored);
    assert_eq!(Registration::from_stored(None), Registration::default());
    let junk = serde_json::json!({"endpoint": "https://web.push.apple.com/x"});
    assert_eq!(
        Registration::from_stored(Some(&junk)),
        Registration::default()
    );
}
