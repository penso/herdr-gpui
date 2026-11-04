use super::*;
use crate::control::protocol::hosts::AccountQuota;

#[test]
fn quota_text_distinguishes_missing_allowance_from_an_exhausted_one() {
    let mut account = AccountQuota {
        login: "octocat".into(),
        quota: None,
        retry_after_seconds: None,
    };
    assert_eq!(
        text::quotas(&[account.clone()]),
        "octocat: GraphQL quota not reported yet"
    );
    account.quota = Some(sheprd_github::quota::Quota {
        limit: 5000,
        used: 5000,
        remaining: 0,
        reset: None,
    });
    account.retry_after_seconds = Some(1739);
    assert_eq!(
        text::quotas(&[account]),
        "octocat: 5000 / 5000 GraphQL points used; 0 remaining; retrying in 29 min"
    );
    assert_eq!(text::quotas(&[]), "No GitHub accounts signed in");
}
