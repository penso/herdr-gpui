use super::super::pr::{pr_info, status};
use super::*;
use crate::{
    control::protocol::hosts::CheckInfo,
    pull_request::{Input, fixture},
    sidebar::layout_tests::REPO_KEY,
};

/// Workspace w3's checkout: the fixture daemon's worktree metadata.
fn input() -> Input {
    Input {
        checkout: None,
        repo_key: Some(REPO_KEY.into()),
        branch: "develop".into(),
    }
}

/// Signed in to GitHub, on this machine's own daemon.
fn signed_in(view: &mut HerdrWindow) {
    view.menu.github = crate::github::Auth::connected_fixture();
    view.live.local_daemon_peer = true;
}

#[test]
fn a_pull_request_maps_to_what_the_command_prints() {
    let info = pr_info(&fixture().unwrap());
    assert_eq!(info.number, 8);
    assert_eq!(info.url, "https://github.com/example/project/pull/8");
    assert_eq!(info.state, "Open");
    assert!(!info.draft);
    assert_eq!(
        (info.head.as_str(), info.base.as_str()),
        ("feature", "main")
    );
    assert_eq!(
        (info.additions, info.deletions, info.changed_files),
        (1730, 31, 16)
    );
    assert_eq!(info.review.as_deref(), Some("Review required"));
    assert_eq!(info.merge_status.as_deref(), Some("Merge blocked"));
    assert_eq!(
        info.checks,
        [
            CheckInfo {
                name: "test".into(),
                outcome: "passed".into()
            },
            CheckInfo {
                name: "ci/lint".into(),
                outcome: "failed".into()
            },
            CheckInfo {
                name: "build".into(),
                outcome: "pending".into()
            },
        ]
    );
    // Absent decisions are left out rather than printed as placeholders.
    let mut quiet = fixture().unwrap();
    quiet.review_decision = crate::pull_request::ReviewDecision::None;
    quiet.merge_state_status = crate::pull_request::MergeState::Unknown;
    let info = pr_info(&quiet);
    assert_eq!((info.review, info.merge_status), (None, None));
}

#[gpui::test]
fn the_callers_branch_reads_the_pull_request_cache(cx: &mut TestAppContext) {
    let (view, cx) = window(cx);
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            // Signed out: nothing to look up with.
            let refused = status(view, &caller(Some("w3")));
            assert_eq!(error_code(&refused), Some(ErrorCode::InvalidRequest));
            signed_in(view);
            // Not looked up yet: still looking, not "no pull request".
            let Response::Host {
                reply: Reply::Pr { pr: None, note },
            } = status(view, &caller(Some("w3")))
            else {
                panic!("expected no pull request yet");
            };
            assert!(note.unwrap().contains("still looking"));
            view.menu
                .pr_cache
                .seed(input(), fixture().unwrap(), std::time::Instant::now());
            let Response::Host {
                reply:
                    Reply::Pr {
                        pr: Some(pr),
                        note: None,
                    },
            } = status(view, &caller(Some("w3")))
            else {
                panic!("expected the seeded pull request");
            };
            assert_eq!(pr.number, 8);
        })
    });
}

#[gpui::test]
fn a_workspace_on_another_host_is_refused_with_how_to_select_it(cx: &mut TestAppContext) {
    let (view, cx) = window(cx);
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            signed_in(view);
            let mut remote = crate::endpoint::Endpoint::new(
                "ssh:devbox".into(),
                "Devbox".into(),
                herdr_client::ConnectTarget::Ssh {
                    target: "me@devbox".into(),
                    session: "default".into(),
                },
                true,
            );
            let mut live = view.live.clone();
            let mut snapshot = (**live.snapshot.as_ref().unwrap()).clone();
            snapshot.workspaces[0].workspace_id = "remote_w".into();
            live.snapshot = Some(std::sync::Arc::new(snapshot));
            remote.live = live;
            view.endpoints.push(remote);
            let Response::Error { code, message } = status(view, &caller(Some("remote_w"))) else {
                panic!("expected a refusal");
            };
            assert_eq!(code, ErrorCode::InvalidRequest);
            assert!(message.contains("host select ssh:devbox"), "{message}");
        })
    });
}

#[gpui::test]
fn comments_load_only_for_the_followed_pull_request(cx: &mut TestAppContext) {
    let (view, cx) = window(cx);
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            signed_in(view);
            view.menu
                .pr_cache
                .seed(input(), fixture().unwrap(), std::time::Instant::now());
            // w3 is not the focused workspace, and nothing follows its PR.
            let target = crate::pr_actions::Target::for_reading(&fixture().unwrap()).unwrap();
            let refused = super::super::pr::loaded(view, &target).unwrap();
            assert_eq!(error_code(&refused), Some(ErrorCode::Failed));
            view.pr_actions.track(Some(target.clone()));
            view.pr_actions
                .load_comments(std::sync::Arc::new(secrecy::SecretString::from("token")))
                .unwrap();
            // Loading: no answer yet, so the request stays parked.
            assert!(super::super::pr::loaded(view, &target).is_none());
        })
    });
}
