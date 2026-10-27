//! A section with nothing to list says what it is for.
use crate::orchestrator::{
    Error, Request, Timing,
    view::{Look, OrchestratorView, list::Empty, rows::Tab},
};
use gpui::{Entity, TestAppContext, VisualTestContext};
use std::sync::Arc;

/// The view, and the folder its repository's data lives in while it is open.
pub(super) fn open(
    cx: &mut TestAppContext,
) -> (
    tempfile::TempDir,
    Entity<OrchestratorView>,
    &mut VisualTestContext,
) {
    let dir = tempfile::tempdir().unwrap();
    let config = crate::config::Config::default();
    let look = Look {
        theme: crate::config::Theme::default(),
        ui: config.ui.clone(),
        mono: config.terminal.clone(),
    };
    let request = Request {
        target: herdr_client::ConnectTarget::Local,
        checkout: dir.path().join("app").to_string_lossy().into_owned(),
        workspace_id: "app".into(),
        token: None,
        data_root: Some(dir.path().join("data")),
        gitlab_store: Default::default(),
        timing: Timing::default(),
    };
    let (view, cx) = cx.add_window_view(|_, cx| OrchestratorView::with_repo(request, look, cx));
    cx.simulate_resize(gpui::size(gpui::px(1100.), gpui::px(600.)));
    (dir, view, cx)
}

#[gpui::test]
fn an_empty_section_explains_itself(cx: &mut TestAppContext) {
    let (_dir, view, cx) = open(cx);
    for tab in [Tab::Issues, Tab::PullRequests] {
        cx.update(|_, cx| {
            view.update(cx, |view, cx| {
                view.repos.clear();
                view.tab = tab;
                view.refresh_rows();
                cx.notify();
            })
        });
        cx.run_until_parked();
        assert!(cx.debug_bounds("orchestrator-empty").is_some());
        assert!(cx.debug_bounds("orchestrator-list").is_some());
    }
}

#[gpui::test]
fn a_project_or_backend_picked_says_the_filters_hide_the_rest(cx: &mut TestAppContext) {
    let (_dir, view, cx) = open(cx);
    for tab in [Tab::Issues, Tab::PullRequests] {
        let empty = cx.update(|_, cx| {
            view.update(cx, |view, _| {
                // Settled, so the list is not still loading.
                for repo in &mut view.repos {
                    repo.snapshot.error = Some(Arc::new(Error::NoDataDirectory));
                }
                view.tab = tab;
                view.refresh_rows();
                let unfiltered = view.empty();
                view.projects.insert("another project".into());
                view.refresh_rows();
                let by_project = view.empty();
                view.projects.clear();
                view.backends.insert(crate::orchestrator::Provider::Github);
                view.refresh_rows();
                let by_backend = view.empty();
                view.backends.clear();
                (unfiltered, by_project, by_backend)
            })
        });
        assert_eq!(empty, (Empty::Nothing, Empty::Filtered, Empty::Filtered));
    }
}
