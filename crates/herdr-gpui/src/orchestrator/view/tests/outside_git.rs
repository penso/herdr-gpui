//! A workspace opened on a folder outside Git is no project: the view
//! neither counts it, offers its pill, nor reports it in a banner.
use super::*;
use crate::orchestrator::{
    Error, Request, Timing,
    view::{Look, OrchestratorView, RepoSpec, repo::Repo, rows::Tab},
};
use gpui::TestAppContext;
use std::sync::Arc;

fn request(dir: &std::path::Path, name: &str) -> Request {
    Request {
        target: herdr_client::ConnectTarget::Local,
        checkout: dir.join(name).to_string_lossy().into_owned(),
        workspace_id: name.into(),
        token: None,
        data_root: Some(dir.join("data")),
        timing: Timing::default(),
    }
}

#[gpui::test]
fn a_folder_outside_git_is_left_out_quietly(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let config = crate::config::Config::default();
    let look = Look {
        theme: crate::config::Theme::default(),
        ui: config.ui.clone(),
        mono: config.terminal.clone(),
    };
    let first = request(dir.path(), "app");
    let second = request(dir.path(), "dock");
    let (view, cx) = cx.add_window_view(|_, cx| OrchestratorView::with_repo(first, look, cx));
    cx.simulate_resize(gpui::size(gpui::px(1100.), gpui::px(600.)));
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.repos.push(Repo::start(RepoSpec {
                key: "dock".into(),
                host: "local".into(),
                request: second,
            }));
            view.repos[0].snapshot.items = Arc::new(vec![item(
                Provider::Github,
                "12",
                "Hero",
                "2026-10-04T00:00:00Z",
            )]);
            view.repos[1].snapshot.error = Some(Arc::new(Error::NotARepository));
            view.tab = Tab::Issues;
            view.refresh_rows();
            cx.notify();
        })
    });
    cx.run_until_parked();
    assert_eq!(cx.update(|_, cx| view.read(cx).projects_named().count()), 1);
    assert!(cx.debug_bounds("orchestrator-banner").is_none());
    assert!(cx.debug_bounds("orchestrator-pill-project-0").is_none());
}

#[gpui::test]
fn a_pick_of_a_folder_outside_git_is_dropped(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let config = crate::config::Config::default();
    let look = Look {
        theme: crate::config::Theme::default(),
        ui: config.ui.clone(),
        mono: config.terminal.clone(),
    };
    let first = request(dir.path(), "app");
    let second = request(dir.path(), "dock");
    let (view, cx) = cx.add_window_view(|_, cx| OrchestratorView::with_repo(first, look, cx));
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.repos.push(Repo::start(RepoSpec {
                key: "dock".into(),
                host: "local".into(),
                request: second,
            }));
            let issue = item(Provider::Github, "12", "Hero", "2026-10-04T00:00:00Z");
            let key = issue.key.canonical();
            view.repos[0].snapshot.items = Arc::new(vec![issue]);
            view.tab = Tab::Issues;
            // Its workspace's issues were asked for before detection ended.
            view.only_project("dock".into(), cx);
            assert!(view.keys().is_empty());
            view.repos[1].snapshot.error = Some(Arc::new(Error::NotARepository));
            view.refresh_rows();
            assert!(view.projects.is_empty());
            assert_eq!(view.keys(), [key]);
        })
    });
}
