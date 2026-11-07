//! The Pull requests list on Azure DevOps: the Status column settles what a
//! pull request is, and the Review column names its reviewers' verdict.
use super::*;
use crate::orchestrator::{
    PrStatus, PullRequest, Request, Review, Timing,
    view::{
        Look, OrchestratorView,
        pr_cells::{review_text, row_tone, standing_word},
        rows::Tab,
    },
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
fn the_review_column_names_every_verdict() {
    let words = |review| review_text(Some(&reviewed(review)), None).map(|(text, _)| text);
    for (review, expected) in [
        (Review::Approved, "approved"),
        (Review::ApprovedWithSuggestions, "approved with suggestions"),
        (Review::WaitingForAuthor, "waiting for author"),
        (Review::Rejected, "rejected"),
        (Review::Required, "review required"),
        (Review::ChangesRequested, "changes requested"),
    ] {
        assert_eq!(words(review).as_deref(), Some(expected));
    }
    assert!(review_text(None, None).is_none());
}

#[test]
fn the_status_column_settles_an_azure_pull_request() {
    let word = |item: &Item, status: Option<&PrStatus>| {
        standing_word(row_tone(item, status), item, status)
    };
    let approved = reviewed(Review::Approved);
    assert_eq!(
        word(&azure_pull(1, "merged"), Some(&approved)).as_deref(),
        Some("merged")
    );
    assert_eq!(
        word(&azure_pull(2, "closed"), None).as_deref(),
        Some("abandoned")
    );
    let mut github = azure_pull(3, "closed");
    github.key.source.provider = Provider::Github;
    assert_eq!(word(&github, None).as_deref(), Some("closed"));
    // Azure DevOps reports no merge state or checks here, so an open one is
    // never called waiting or ready: the review column has its verdict.
    for review in [Review::Approved, Review::Rejected, Review::Required] {
        assert_eq!(word(&azure_pull(4, "open"), Some(&reviewed(review))), None);
    }
}

#[test]
fn yours_takes_authorship_from_the_caller() {
    let yours = |authored, review_requested| Yours {
        authored,
        review_requested,
    };
    let asked = PrStatus {
        requested: vec!["Dev One".into()],
        ..PrStatus::default()
    };
    let login = Some("Dev One");
    assert!(yours(false, false).holds(false, None, None));
    assert!(yours(true, false).holds(true, None, login));
    assert!(!yours(true, false).holds(false, Some(&asked), login));
    assert!(yours(true, true).holds(false, Some(&asked), login));
    assert!(!yours(true, false).holds(true, None, None));
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
fn an_azure_pull_request_row_draws_its_review(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let request = Request {
        target: sheprd_client::ConnectTarget::Local,
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
    let selector: &'static str = Box::leak(format!("orchestrator-review-{key}").into_boxed_str());
    let cell = cx
        .debug_bounds(selector)
        .unwrap_or_else(|| panic!("the review cell is not drawn"));
    assert!(cell.size.width > gpui::px(0.));
}
