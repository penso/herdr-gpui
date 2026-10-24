//! A review asks for its worktree at the pull request's head, which its
//! prompt names, fetched from the remote the pull request was listed from.
use super::{args::open, *};
use crate::orchestrator::PullHead;

fn request(
    view: &gpui::Entity<OrchestratorView>,
    cx: &mut gpui::VisualTestContext,
) -> Option<crate::orchestrator::DispatchRequest> {
    cx.update(|_, cx| view.update(cx, |view, cx| view.dispatch_request(cx)))
        .map(|ready| ready.request)
}

#[gpui::test]
fn a_review_starts_at_the_head_its_prompt_names(cx: &mut TestAppContext) {
    let (view, cx, _dir) = view_in_window(cx);
    open(&view, cx, true);
    let request = request(&view, cx).unwrap();
    let head = "b".repeat(40);
    assert_eq!(
        request.pull_request,
        Some(PullHead {
            number: 12,
            sha: head.clone(),
            remote: "upstream".into(),
        })
    );
    assert!(request.prompt.contains(&format!("Head SHA: {head}")));
    // Its branch is the head's own, so a later push is not reviewed on an
    // earlier review's branch.
    assert!(
        request
            .branch
            .starts_with("agent/review-12-clarify-headings-bbbbbbb-"),
        "{}",
        request.branch
    );
}

#[gpui::test]
fn a_dispatch_on_a_pull_request_still_starts_at_the_main_checkout(cx: &mut TestAppContext) {
    let (view, cx, _dir) = view_in_window(cx);
    open(&view, cx, false);
    let request = request(&view, cx).unwrap();
    assert_eq!((request.pull_request, request.base), (None, None));
}

#[gpui::test]
fn a_review_without_a_remote_to_fetch_from_is_refused(cx: &mut TestAppContext) {
    let (view, cx, _dir) = view_in_window(cx);
    open(&view, cx, true);
    cx.update(|_, cx| view.update(cx, |view, _| view.repo_mut().snapshot.repo = None));
    assert!(request(&view, cx).is_none());
    let problem = cx.update(|_, cx| view.read(cx).dialog.as_ref().unwrap().problem);
    assert_eq!(
        problem,
        Some("This repository has no remote to fetch the pull request from.")
    );
}
