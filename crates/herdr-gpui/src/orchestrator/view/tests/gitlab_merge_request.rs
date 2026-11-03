//! A GitLab merge request is listed with the pull requests, but the pages
//! that read GitHub (status, conversation, checks, merge, `gh` review) are
//! not offered for it.
use super::*;
use crate::orchestrator::view::{DetailTab, rows::Tab};
use crate::orchestrator::{Access, PullRequest};
use std::sync::Arc;

fn pull_request(provider: Provider, id: &str, identifier: &str) -> Item {
    let mut item = item(provider, id, "Add merge requests", "2026-10-04T00:00:00Z");
    item.identifier = identifier.into();
    item.pull_request = Some(PullRequest {
        number: 7,
        additions: None,
        deletions: None,
        base_ref: "main".into(),
        head_ref: "feature/mrs".into(),
        base_sha: String::new(),
        head_sha: String::new(),
        head_repository: None,
    });
    item
}

#[gpui::test]
fn a_merge_request_has_no_github_pages_or_review(cx: &mut gpui::TestAppContext) {
    let (_dir, view, cx) = empty::open(cx);
    let merge_request = pull_request(Provider::Gitlab, "pr/7", "!7");
    let github = pull_request(Provider::Github, "pr/8", "#8");
    let (merge_key, github_key) = (merge_request.key.canonical(), github.key.canonical());
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            let repo = view.repo_mut();
            repo.snapshot.access = Some(Access::ReadWrite);
            repo.snapshot.items = Arc::new(vec![merge_request, github]);
            view.tab = Tab::PullRequests;
            view.refresh_rows();
            assert_eq!(view.keys().len(), 2, "both are in the Pull requests list");

            view.open_detail(merge_key.clone(), cx);
            assert_eq!(
                view.detail.as_ref().map(|detail| detail.tab),
                Some(DetailTab::Description)
            );
        })
    });
    cx.run_until_parked();
    assert!(cx.debug_bounds("orchestrator-status").is_some());
    assert!(cx.debug_bounds("orchestrator-tab-Description").is_some());
    assert!(cx.debug_bounds("orchestrator-tab-Conversation").is_none());
    assert!(cx.debug_bounds("orchestrator-tab-Checks").is_none());
    assert!(cx.debug_bounds("orchestrator-detail-review").is_none());
    assert!(cx.debug_bounds("orchestrator-detail-dispatch").is_none());

    cx.update(|_, cx| {
        view.update(cx, |view, cx| view.open_detail(github_key, cx));
    });
    cx.run_until_parked();
    cx.update(|_, cx| {
        assert_eq!(
            view.read(cx).detail.as_ref().map(|detail| detail.tab),
            Some(DetailTab::Conversation)
        );
    });
    assert!(cx.debug_bounds("orchestrator-tab-Conversation").is_some());
    assert!(cx.debug_bounds("orchestrator-detail-review").is_some());
}

#[test]
fn an_unlisted_merge_request_is_called_by_its_bang_number() {
    assert_eq!(key_label("gitlab:gitlab.com:g/app:pr/7"), "!7");
    assert_eq!(key_label("gitlab:gitlab.com:g/app:7"), "#7");
    assert_eq!(key_label("github:github.com:g/app:pr/7"), "#7");
}

#[gpui::test]
fn a_gitlab_repository_does_not_offer_the_github_mine_identity(cx: &mut gpui::TestAppContext) {
    let (_dir, view, cx) = empty::open(cx);
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            let repo = view.repo_mut();
            repo.snapshot.repo = Some(crate::orchestrator::repo::parse(
                &herdr_client::ConnectTarget::Local,
                "common\t/repo/.git\nmain\t/repo\nremote\torigin\tgit@gitlab.com:penso/app.git\n",
            ).unwrap());
            repo.login = Some("github-only-login".into());
            assert_eq!(repo.reading_login(), None);
        });
    });
}

#[test]
fn a_merge_request_cannot_inherit_github_checks() {
    let mr = pull_request(Provider::Gitlab, "pr/7", "!7");
    let github = pull_request(Provider::Github, "pr/7", "#7");
    let status = crate::orchestrator::PrStatus {
        review: None,
        requested: Vec::new(),
        checks: Vec::new(),
    };
    let statuses =
        std::collections::HashMap::from([((mr.key.source.repository.clone(), 7), status)]);
    assert!(status_of(&github, &statuses).is_some());
    assert!(status_of(&mr, &statuses).is_none());
}
