use super::*;
use chrono::{DateTime, Utc};
use serde_json::json;

fn reply(status: u16, headers: &[(&str, &str)]) -> ureq::http::Response<ureq::Body> {
    let mut reply = ureq::http::Response::builder().status(status);
    for (name, value) in headers {
        reply = reply.header(*name, *value);
    }
    reply.body(ureq::Body::builder().data(Vec::new())).unwrap()
}

fn at(epoch: i64) -> DateTime<Utc> {
    DateTime::from_timestamp(epoch, 0).unwrap()
}

#[test]
fn an_exhausted_quota_is_a_rate_limit_with_its_reset_not_a_refusal() {
    let reset = Utc::now().timestamp() + 600;
    let error = response::<Value>(
        "test",
        reply(
            403,
            &[
                ("x-ratelimit-remaining", "0"),
                ("x-ratelimit-reset", &reset.to_string()),
            ],
        ),
    )
    .unwrap_err();
    assert!(
        matches!(error, Error::RateLimit { reset: Some(at) } if at.timestamp() == reset),
        "{error:?}"
    );
    assert!(error.to_string().contains("Retry after "), "{error}");
    assert_eq!(kind(&error), "rate_limit");

    // A secondary limit sends only a delay.
    let error = response::<Value>("test", reply(403, &[("retry-after", "120")])).unwrap_err();
    assert!(matches!(error, Error::RateLimit { reset: Some(_) }));

    // 429 is always a rate limit; without a time it says so.
    let error = response::<Value>("test", reply(429, &[])).unwrap_err();
    assert!(matches!(error, Error::RateLimit { reset: None }));
    assert_eq!(error.to_string(), "GitHub rate limit reached. Retry later.");
}

#[test]
fn a_403_with_quota_left_stays_a_permission_refusal() {
    for headers in [&[][..], &[("x-ratelimit-remaining", "4999")]] {
        let error = response::<Value>("test", reply(403, headers)).unwrap_err();
        assert!(matches!(error, Error::Forbidden), "{error:?}");
    }
}

#[test]
fn graphql_rate_limits_carry_the_reset_time() {
    let errors = json!([{"type": "RATE_LIMITED", "message": "API rate limit exceeded"}]);
    let error = query_error(&errors, Some("a/b".into()), "oauth_app", Some(at(2_000)));
    assert!(matches!(error, Error::RateLimit { reset: Some(reset) } if reset == at(2_000)));
}

#[test]
fn a_repository_the_sign_in_cannot_see_says_which_and_why() {
    let errors = json!([{
        "type": "NOT_FOUND",
        "path": ["repository"],
        "message": "Could not resolve to a Repository with the name 'acme/hidden'."
    }]);
    let error = query_error(&errors, Some("acme/hidden".into()), "github_app_user", None);
    assert!(matches!(
        &error,
        Error::RepositoryHidden { repository, app_token: true } if repository == "acme/hidden"
    ));
    let text = error.to_string();
    assert!(text.contains("acme/hidden"), "{text}");
    assert!(text.contains("installed on"), "{text}");
    assert_eq!(kind(&error), "repository_hidden");

    let error = query_error(&errors, Some("acme/hidden".into()), "oauth_app", None);
    assert!(matches!(
        error,
        Error::RepositoryHidden {
            app_token: false,
            ..
        }
    ));
    assert!(!error.to_string().contains("installed on"));
}

#[test]
fn other_graphql_errors_show_github_reason() {
    // A missing pull request inside a visible repository is not a hidden one.
    let nested = json!([{
        "type": "NOT_FOUND",
        "path": ["repository", "pullRequest"],
        "message": "Could not resolve to a PullRequest with the number of 9."
    }]);
    assert!(matches!(
        query_error(&nested, Some("a/b".into()), "github_app_user", None),
        Error::Rejected(message) if message.contains("PullRequest")
    ));
    let forbidden =
        json!([{"type": "FORBIDDEN", "message": "Resource not accessible by integration"}]);
    assert_eq!(
        query_error(&forbidden, None, "github_app_user", None).to_string(),
        "GitHub refused the request: Resource not accessible by integration"
    );
    assert!(matches!(
        query_error(&json!([]), None, "unknown", None),
        Error::Query
    ));
}

#[test]
fn graphql_pauses_last_until_the_quota_refills_within_bounds() {
    let now = at(1_000);
    assert_eq!(pause_until(None, now), Duration::from_secs(3600));
    assert_eq!(
        pause_until(Some(at(2_200)), now),
        Duration::from_secs(1_200)
    );
    assert_eq!(pause_until(Some(at(1_010)), now), Duration::from_secs(300));
    assert_eq!(pause_until(Some(at(900)), now), Duration::from_secs(3600));
    assert_eq!(
        pause_until(Some(at(1_000 + 200_000)), now),
        Duration::from_secs(86_400)
    );
}
