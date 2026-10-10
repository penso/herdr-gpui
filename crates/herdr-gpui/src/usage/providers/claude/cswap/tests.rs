#![allow(clippy::unwrap_used)]

use super::*;

const LIST: &str = r#"{"schemaVersion":1,"activeAccountNumber":2,"accounts":[
  {"number":1,"email":"work@example.com","alias":"work","active":false,"usageStatus":"ok",
   "usage":{"fiveHour":{"pct":25.0,"resetsAt":"2026-10-04T23:29:59Z","countdown":"3h"},
            "sevenDay":{"pct":16.0,"resetsAt":"2026-10-08T17:59:59Z","expectedPct":40.1},
            "scoped":[{"name":"Fable","pct":80.0,"resetsAt":"2026-10-08T17:59:59Z"}]},
   "usageAgeSeconds":42.0,"loginExpiresAt":"2026-12-01T00:00:00Z"},
  {"number":2,"email":"me@example.com","active":true,"usageStatus":"ok",
   "usage":{"fiveHour":{"pct":5.0}}},
  {"number":3,"email":"old@example.com","alias":"","active":false,"usageStatus":"token_expired",
   "usage":null,"lastGoodUsage":{"sevenDay":{"pct":99.5}},"disabled":true},
  {"number":4,"email":"new@example.com","active":false,"usageStatus":"some_future_state",
   "usage":null}]}"#;

#[test]
fn splits_the_active_account_from_the_others() {
    let accounts = parse(LIST).unwrap();
    let active = accounts.active.unwrap();
    assert_eq!(active.email, "me@example.com");
    assert_eq!(active.windows.len(), 1);
    assert_eq!(
        accounts.others.iter().map(Other::name).collect::<Vec<_>>(),
        ["work", "old@example.com", "new@example.com"]
    );
}

#[test]
fn reads_session_weekly_and_model_windows() {
    let accounts = parse(LIST).unwrap();
    let work = &accounts.others[0];
    assert_eq!(work.note(), None);
    assert_eq!(
        work.windows
            .iter()
            .map(|window| (window.kind.clone(), window.percent(), window.length))
            .collect::<Vec<_>>(),
        [
            (Kind::Session, 25, Some(SESSION)),
            (Kind::Weekly, 16, Some(WEEK)),
            (Kind::Named("Fable".into()), 80, Some(WEEK)),
        ]
    );
    assert_eq!(
        work.windows[0].resets_at,
        Timestamp::Text("2026-10-04T23:29:59Z".into()).time()
    );
}

#[test]
fn falls_back_to_the_last_good_reading() {
    let accounts = parse(LIST).unwrap();
    let old = &accounts.others[1];
    assert_eq!(old.status, Status::TokenExpired);
    assert!(old.stale);
    assert!(old.disabled);
    assert_eq!(old.note(), Some("token expired"));
    assert_eq!(old.windows[0].kind, Kind::Weekly);
    assert_eq!(old.windows[0].percent(), 100);
}

#[test]
fn unknown_states_are_unavailable_without_windows() {
    let accounts = parse(LIST).unwrap();
    let new = &accounts.others[2];
    assert_eq!(new.status, Status::Unknown);
    assert!(!new.stale);
    assert!(new.windows.is_empty());
    assert_eq!(new.note(), Some("unavailable"));
}

#[test]
fn errors_and_other_schemas_list_nothing() {
    let error = r#"{"schemaVersion":1,"error":{"type":"NoAccounts","message":"none"}}"#;
    assert_eq!(parse(error).unwrap(), Accounts::default());
    let future = LIST.replace("\"schemaVersion\":1", "\"schemaVersion\":2");
    assert_eq!(parse(&future).unwrap(), Accounts::default());
    assert!(parse("cswap: command not found").is_err());
}
