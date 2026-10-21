//! Switching lists leaves the open item behind with everything it held.
use super::*;
use crate::orchestrator::{PullRequest, service::PullRequestLookup, view::rows::Tab};
use std::sync::Arc;

#[gpui::test]
fn switching_lists_closes_the_open_pull_request(cx: &mut gpui::TestAppContext) {
    let (_dir, view, cx) = empty::open(cx);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            let looked_up = crate::pull_request::fixture().unwrap();
            let mut pr = item(Provider::Github, "pr/8", "Menus", "2026-10-04T00:00:00Z");
            pr.pull_request = Some(PullRequest {
                number: looked_up.number,
                additions: None,
                deletions: None,
                base_ref: "main".into(),
                head_ref: "feature".into(),
                base_sha: String::new(),
                head_sha: String::new(),
                head_repository: None,
            });
            let key = pr.key.canonical();
            let repo = view.repo_mut();
            repo.snapshot.items = Arc::new(vec![pr]);
            repo.snapshot.pull_request = Some(PullRequestLookup {
                number: looked_up.number,
                result: Arc::new(Ok(Some(looked_up))),
            });
            view.show_list(Tab::PullRequests, cx);
            view.open_detail(key, cx);
            view.merge_open = true;
            assert!(view.detail.is_some());
            assert!(view.pr.target().is_some());

            view.show_list(Tab::Issues, cx);
            assert!(view.detail.is_none());
            assert!(!view.merge_open);
            assert!(view.pr.target().is_none());
        })
    });
}
