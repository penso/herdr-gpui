use super::*;
use herdr_github::quota::Quota;

fn accounts(view: &HerdrWindow) -> Vec<crate::control::protocol::hosts::AccountQuota> {
    let Response::Host {
        reply: Reply::PrQuota { accounts },
    } = super::super::pr::quota(view)
    else {
        panic!("missing quota reply")
    };
    accounts
}

#[gpui::test]
fn a_known_host_quota_and_header_pause_survive_an_unobserved_main_account(cx: &mut TestAppContext) {
    let (view, cx) = window(cx);
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            let quota = Quota {
                limit: 5000,
                used: 5000,
                remaining: 0,
                reset: Some(chrono::Utc::now() + chrono::TimeDelta::minutes(29)),
            };
            for known_main in [false, true] {
                let mut known = crate::github::Auth::connected_fixture();
                known.seed_quota(quota);
                let missing = crate::github::Auth::connected_fixture();
                let (main, host) = if known_main {
                    (known, missing)
                } else {
                    (missing, known)
                };
                view.menu.github = main;
                view.menu.github_hosts.insert("ssh:quota".into(), host);
                let observed = accounts(view);
                let [account] = observed.as_slice() else {
                    panic!("duplicate login was not merged")
                };
                assert_eq!(account.quota, Some(quota));
                assert!(
                    account
                        .retry_after_seconds
                        .is_some_and(|seconds| (1730..=1740).contains(&seconds))
                );
            }
        })
    });
}

#[gpui::test]
fn duplicate_logins_keep_a_whole_recent_observation_and_the_longest_pause(cx: &mut TestAppContext) {
    let (view, cx) = window(cx);
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            let reset = chrono::Utc::now() + chrono::TimeDelta::minutes(10);
            let older = Quota {
                limit: 5000,
                used: 5000,
                remaining: 0,
                reset: Some(reset),
            };
            for newer in [
                Quota {
                    used: 5001,
                    ..older
                },
                Quota {
                    used: 10,
                    remaining: 4990,
                    reset: Some(reset + chrono::TimeDelta::hours(1)),
                    ..older
                },
            ] {
                for reverse in [false, true] {
                    let mut main = crate::github::Auth::connected_fixture();
                    let mut host = crate::github::Auth::connected_fixture();
                    main.seed_quota(if reverse { newer } else { older });
                    host.seed_quota(if reverse { older } else { newer });
                    view.menu.github = main;
                    view.menu.github_hosts.insert("ssh:quota".into(), host);
                    let observed = accounts(view);
                    let [account] = observed.as_slice() else {
                        panic!("duplicate login was not merged")
                    };
                    assert_eq!(account.quota, Some(newer));
                    assert!(
                        account
                            .retry_after_seconds
                            .is_some_and(|seconds| (590..=600).contains(&seconds))
                    );
                }
            }
        })
    });
}
