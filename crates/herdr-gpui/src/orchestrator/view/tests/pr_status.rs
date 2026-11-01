//! The Pull requests list's Status column: the state, and for an open pull
//! request where its reviews stand.
use super::*;
use crate::orchestrator::{
    PrStatus, PullRequest, Request, Review, Timing,
    view::{Look, OrchestratorView, list::status_text, rows::Tab},
};
use gpui::TestAppContext;
use std::{collections::HashMap, sync::Arc};

fn azure_pull(id: u64, state: &str) -> Item {
    let mut pull = item(
        Provider::Azure,
        &format!("pr/{id}"),
        "Change",
        "2026-10-04T00:00:00Z",
    );
    pull.key.source = SourceKey {
        provider: Provider::Azure,
        host: "dev.azure.com".into(),
        repository: "contoso/Fabrikam/web".into(),
    };
    pull.state = state.into();
    pull.pull_request = Some(PullRequest {
        number: id,
        additions: None,
        deletions: None,
        base_ref: "main".into(),
        head_ref: format!("feature/{id}"),
        base_sha: String::new(),
        head_sha: String::new(),
        head_repository: None,
    });
    pull
}

fn reviewed(review: Review) -> PrStatus {
    PrStatus {
        review: Some(review),
        ..PrStatus::default()
    }
}

#[test]
fn the_status_names_the_state_then_the_review() {
    let text = |item: &Item, status: Option<&PrStatus>| {
        status_text(item, status, None).map(|(text, _)| text)
    };
    let approved = reviewed(Review::Approved);
    // A settled or draft pull request shows its state, whatever its reviews.
    assert_eq!(
        text(&azure_pull(1, "merged"), Some(&approved)).as_deref(),
        Some("merged")
    );
    assert_eq!(
        text(&azure_pull(2, "closed"), None).as_deref(),
        Some("abandoned")
    );
    let mut github = azure_pull(3, "closed");
    github.key.source.provider = Provider::Github;
    assert_eq!(text(&github, None).as_deref(), Some("closed"));
    assert_eq!(
        text(&azure_pull(4, "draft"), None).as_deref(),
        Some("draft")
    );
    // An open one shows where its reviews stand, else that it is open.
    let open = azure_pull(5, "open");
    assert_eq!(text(&open, None).as_deref(), Some("open"));
    for (review, words) in [
        (Review::Approved, "approved"),
        (Review::ApprovedWithSuggestions, "approved with suggestions"),
        (Review::WaitingForAuthor, "waiting for author"),
        (Review::Rejected, "rejected"),
        (Review::Required, "review required"),
        (Review::ChangesRequested, "changes requested"),
    ] {
        assert_eq!(text(&open, Some(&reviewed(review))).as_deref(), Some(words));
    }
    // An issue has no status cell.
    let mut issue = azure_pull(6, "open");
    issue.pull_request = None;
    assert!(text(&issue, None).is_none());
}

#[test]
fn azure_verdicts_group_as_githubs_do() {
    let mut open = azure_pull(1, "open");
    open.author = Some("Dev One".into());
    let next = |review| Next::of(&open, Some(&reviewed(review)), Some("Dev One"));
    // The reader's own, waiting on them.
    assert_eq!(next(Review::WaitingForAuthor), Next::Fix);
    assert_eq!(next(Review::Rejected), Next::Fix);
    assert_eq!(next(Review::ApprovedWithSuggestions), Next::Merge);
}

#[gpui::test]
fn an_azure_pull_request_row_draws_its_status(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let request = Request {
        target: herdr_client::ConnectTarget::Local,
        checkout: dir.path().join("web").to_string_lossy().into_owned(),
        workspace_id: "w1".into(),
        token: None,
        data_root: Some(dir.path().join("data")),
        gitlab_store: Default::default(),
        timing: Timing::default(),
    };
    let config = crate::config::Config::default();
    let look = Look {
        theme: crate::config::Theme::default(),
        ui: config.ui.clone(),
        mono: config.terminal.clone(),
    };
    let (view, cx) = cx.add_window_view(|_, cx| OrchestratorView::with_repo(request, look, cx));
    cx.simulate_resize(gpui::size(gpui::px(1400.), gpui::px(600.)));
    let pull = azure_pull(7, "open");
    let key = pull.key.canonical();
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            let snapshot = &mut view.repo_mut().snapshot;
            snapshot.items = Arc::new(vec![pull]);
            snapshot.pr_status = Arc::new(HashMap::from([(
                ("contoso/Fabrikam/web".to_owned(), 7),
                reviewed(Review::WaitingForAuthor),
            )]));
            view.tab = Tab::PullRequests;
            view.refresh_rows();
            cx.notify();
        })
    });
    cx.run_until_parked();
    let selector: &'static str = Box::leak(format!("orchestrator-status-{key}").into_boxed_str());
    let cell = cx.debug_bounds(selector).expect("the status cell is drawn");
    assert!(cell.size.width > gpui::px(0.));
}
