use super::*;
use chrono::{DateTime, Utc};
use serde_json::json;

fn aliases() -> Vec<(String, String)> {
    vec![
        ("p0".into(), "acme/seen".into()),
        ("p1".into(), "acme/hidden".into()),
    ]
}

fn now() -> DateTime<Utc> {
    DateTime::from_timestamp(1_000, 0).unwrap()
}

#[test]
fn an_error_under_one_alias_fails_only_that_checkout() {
    let result = json!({
        "data": {"p0": {"pullRequests": {"nodes": []}}, "p1": null},
        "errors": [{"type": "NOT_FOUND", "path": ["p1"], "message": "Could not resolve"}],
    });
    let mut cooldown = None;
    let answers = split_aliases(
        result,
        &aliases(),
        "github_app_user",
        None,
        now(),
        &mut cooldown,
    )
    .unwrap();
    assert_eq!(
        answers[0].as_ref().unwrap(),
        &json!({"data": {"repository": {"pullRequests": {"nodes": []}}}})
    );
    assert!(matches!(
        &answers[1],
        Err(crate::Error::GitHub(Error::RepositoryHidden { repository, app_token: true }))
            if repository == "acme/hidden"
    ));
    assert!(cooldown.is_none(), "one hidden repository pauses nothing");
}

#[test]
fn a_rate_limit_or_an_unclaimed_error_fails_the_whole_request() {
    let limited = json!({
        "data": {"p0": null, "p1": null},
        "errors": [{"type": "RATE_LIMITED", "path": ["p0"], "message": "API rate limit"}],
    });
    let mut cooldown = None;
    let reset = Some(DateTime::from_timestamp(2_000, 0).unwrap());
    assert!(matches!(
        split_aliases(limited, &aliases(), "oauth_app", reset, now(), &mut cooldown),
        Err(crate::Error::GitHub(Error::RateLimit { reset: at })) if at == reset
    ));
    assert_eq!(cooldown, Some(Duration::from_secs(1_000)));

    let unclaimed = json!({"errors": [{"message": "Something went wrong"}]});
    let mut cooldown = None;
    assert!(matches!(
        split_aliases(unclaimed, &aliases(), "oauth_app", None, now(), &mut cooldown),
        Err(crate::Error::GitHub(Error::Rejected(message))) if message == "Something went wrong"
    ));
    assert!(cooldown.is_none());
}

#[test]
fn a_forbidden_alias_pauses_the_account_but_fails_only_itself() {
    let result = json!({
        "data": {"p0": {"pullRequests": {"nodes": []}}, "p1": null},
        "errors": [{"type": "FORBIDDEN", "path": ["p1", "pullRequests"], "message": "SAML enforcement"}],
    });
    let mut cooldown = None;
    let answers =
        split_aliases(result, &aliases(), "oauth_app", None, now(), &mut cooldown).unwrap();
    assert!(answers[0].is_ok());
    assert!(answers[1].is_err());
    assert_eq!(cooldown, Some(Duration::from_secs(3600)));
}
