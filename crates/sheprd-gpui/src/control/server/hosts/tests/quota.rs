use super::*;

#[gpui::test]
fn quota_reports_cached_allowance_and_pause_without_a_pull_request(cx: &mut TestAppContext) {
    let (view, cx) = window(cx);
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            view.menu.github = crate::github::Auth::connected_fixture();
            view.menu.github.seed_quota(sheprd_github::quota::Quota {
                limit: 5000,
                used: 5000,
                remaining: 0,
                reset: None,
            });
            view.menu.pr_cache.seed_rate_limit(
                "fixture-user",
                std::time::Instant::now() + std::time::Duration::from_secs(1740),
            );
            let response = super::super::pr::quota(view);
            let Response::Host {
                reply: Reply::PrQuota { accounts },
            } = response
            else {
                panic!("missing quota reply")
            };
            let account = &accounts[0];
            assert_eq!(account.login, "fixture-user");
            assert_eq!(account.quota.unwrap().remaining, 0);
            assert!(
                account
                    .retry_after_seconds
                    .is_some_and(|seconds| seconds <= 1740)
            );
            let encoded = serde_json::to_value(&accounts).unwrap();
            assert_eq!(encoded[0]["quota"]["limit"], 5000);
        })
    });
}
