use super::*;
use crate::orchestrator::{Error, PullRequest, view::list::Empty};
use std::sync::Arc;

#[gpui::test]
fn pr_toggles_explain_why_loaded_pull_requests_are_hidden(cx: &mut gpui::TestAppContext) {
    let (_dir, view, cx) = empty::open(cx);
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            let mut pr = item(Provider::Github, "pr/1", "Fix", "2026-10-04T00:00:00Z");
            pr.pull_request = Some(PullRequest {
                number: 1,
                additions: None,
                deletions: None,
                base_ref: "main".into(),
                head_ref: "fix".into(),
                base_sha: String::new(),
                head_sha: String::new(),
                head_repository: None,
            });
            let key = pr.key.canonical();
            let repo = &mut view.repos[0];
            repo.snapshot.error = Some(Arc::new(Error::NoDataDirectory));
            repo.snapshot.items = Arc::new(vec![pr]);
            repo.login = Some("another-author".into());
            view.tab = Tab::PullRequests;
            view.refresh_rows();
            assert_eq!(view.keys(), std::slice::from_ref(&key));

            view.filters.has_run = true;
            view.refresh_rows();
            assert!(view.keys().is_empty());
            assert_eq!(view.empty(), Empty::Filtered);

            view.filters.has_run = false;
            view.yours.authored = true;
            view.refresh_rows();
            assert!(view.keys().is_empty());
            assert_eq!(view.empty(), Empty::Filtered);

            view.yours.authored = false;
            view.yours.review_requested = true;
            view.refresh_rows();
            assert!(view.keys().is_empty());
            assert_eq!(view.empty(), Empty::Filtered);
            view.yours.review_requested = false;
            view.yours.authored = true;

            view.tab = Tab::Issues;
            view.refresh_rows();
            assert_eq!(view.empty(), Empty::Nothing);

            view.tab = Tab::PullRequests;
            view.yours.authored = false;
            view.refresh_rows();
            assert_eq!(view.keys(), [key]);
            view.repos[0].snapshot.items = Arc::default();
            view.refresh_rows();
            assert_eq!(view.empty(), Empty::Nothing);
        })
    });
}
