use super::*;
use crate::apns::tokens::MAX_ACTIVITIES;
use secrecy::ExposeSecret;

#[test]
fn parses_the_api_request() {
    let body = format!(
        r#"{{"push_to_start":"{}","activities":[{{"pane":"w1-p3","token":"{}"}}]}}"#,
        "aa".repeat(40),
        "bb".repeat(80)
    );
    let registration = Registration::parse(body.as_bytes()).unwrap();
    assert_eq!(
        registration
            .push_to_start
            .as_ref()
            .map(ExposeSecret::expose_secret),
        Some("aa".repeat(40).as_str())
    );
    assert_eq!(
        registration
            .activity("w1-p3")
            .map(ExposeSecret::expose_secret),
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
        activity_detail: Default::default(),
        alert: None,
        notify_key: None,
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
        activity_detail: Default::default(),
        alert: None,
        notify_key: None,
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
        activity_detail: Default::default(),
        alert: None,
        notify_key: None,
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
        activity_detail: Default::default(),
        alert: None,
        notify_key: None,
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
        activity_detail: Default::default(),
        alert: None,
        notify_key: None,
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

#[test]
fn a_phone_keeps_its_activities_generic_until_it_says_otherwise() {
    use crate::app::config::ActivityDetail;

    let generic = Registration::parse(br#"{"activity_detail":"generic"}"#).unwrap();
    assert_eq!(generic.activity_detail, ActivityDetail::Generic);

    let mut stored = Registration {
        push_to_start: Some(DeviceToken::parse(&"ab".repeat(32)).unwrap()),
        ..Registration::default()
    };
    stored.merge(generic);
    let kept = stored.to_stored().unwrap();
    assert_eq!(
        Registration::from_stored(kept.as_ref()).activity_detail,
        ActivityDetail::Generic
    );

    // The app sends its whole choice each time: no field is the default.
    stored.merge(Registration::parse(b"{}").unwrap());
    assert_eq!(stored.activity_detail, ActivityDetail::Detailed);
    let kept = stored.to_stored().unwrap().unwrap();
    assert!(kept.get("activity_detail").is_none());
    assert!(Registration::parse(br#"{"activity_detail":"everything"}"#).is_err());
}

#[test]
fn a_detail_choice_alone_registers_nothing() {
    let registration = Registration::parse(br#"{"activity_detail":"generic"}"#).unwrap();
    assert!(registration.is_empty());
}

#[test]
fn printing_a_registration_shows_no_token() {
    let hex = "ab".repeat(32);
    let registration = Registration {
        alert: Some(DeviceToken::parse(&hex).unwrap()),
        push_to_start: Some(DeviceToken::parse(&hex).unwrap()),
        ..Registration::default()
    };
    let printed = format!("{registration:?}");
    assert!(!printed.contains(&hex), "{printed}");
}
