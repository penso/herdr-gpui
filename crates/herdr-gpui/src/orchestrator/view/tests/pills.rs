//! Pills narrow a list of several repositories by backend and by project.
use super::*;
use crate::orchestrator::{
    Request, Timing,
    view::{Look, OrchestratorView, RepoSpec, repo::Repo, rows::Tab},
};
use gpui::{Modifiers, TestAppContext};
use std::sync::Arc;

fn request(dir: &std::path::Path, name: &str) -> Request {
    Request {
        target: herdr_client::ConnectTarget::Local,
        checkout: dir.join(name).to_string_lossy().into_owned(),
        workspace_id: name.into(),
        token: None,
        data_root: Some(dir.join("data")),
        gitlab_store: Default::default(),
        timing: Timing::default(),
    }
}

#[gpui::test]
fn pills_narrow_the_list_by_backend_and_by_project(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let config = crate::config::Config::default();
    let look = Look {
        theme: crate::config::Theme::default(),
        ui: config.ui.clone(),
        mono: config.terminal.clone(),
    };
    let first = request(dir.path(), "app");
    let second = request(dir.path(), "site");
    let (view, cx) = cx.add_window_view(|_, cx| OrchestratorView::with_repo(first, look, cx));
    cx.simulate_resize(gpui::size(gpui::px(1100.), gpui::px(600.)));
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.repos.push(Repo::start(RepoSpec {
                key: "site".into(),
                host: "local".into(),
                request: second,
            }));
            view.repos[0].snapshot.items = Arc::new(vec![
                item(Provider::Beads, "hg-1", "Store", "2026-10-03T00:00:00Z"),
                item(Provider::Github, "377", "Icons", "2026-10-02T00:00:00Z"),
            ]);
            view.repos[1].snapshot.items = Arc::new(vec![item(
                Provider::Github,
                "12",
                "Hero",
                "2026-10-04T00:00:00Z",
            )]);
            view.tab = Tab::Issues;
            view.refresh_rows();
            cx.notify();
        })
    });
    cx.run_until_parked();
    let listed = |cx: &mut gpui::VisualTestContext| {
        cx.update(|_, cx| {
            view.read(cx)
                .keys()
                .into_iter()
                .map(|key| key.rsplit(':').next().unwrap_or_default().to_owned())
                .collect::<Vec<_>>()
        })
    };
    // Both repositories, newest first.
    assert_eq!(listed(cx), ["12", "hg-1", "377"]);
    // Each row draws its own repository's item, though both repositories
    // have an item at the same place.
    let drawn = |cx: &mut gpui::VisualTestContext, repo: usize| {
        let key = cx.update(|_, cx| view.read(cx).repos[repo].snapshot.items[0].key.canonical());
        let selector: &'static str = Box::leak(format!("orchestrator-row-{key}").into_boxed_str());
        cx.debug_bounds(selector).is_some()
    };
    assert!(drawn(cx, 0) && drawn(cx, 1));
    let click = |cx: &mut gpui::VisualTestContext, selector: &'static str| {
        let at = cx
            .debug_bounds(selector)
            .unwrap_or_else(|| panic!("{selector}"))
            .center();
        cx.simulate_click(at, Modifiers::none());
        cx.run_until_parked();
    };
    click(cx, "orchestrator-pill-backend-Beads");
    assert_eq!(listed(cx), ["hg-1"]);
    click(cx, "orchestrator-pill-backend-GitHub");
    assert_eq!(listed(cx), ["12", "hg-1", "377"]);
    click(cx, "orchestrator-pill-backend-Beads");
    click(cx, "orchestrator-pill-backend-GitHub");
    // A project narrows to its repository's items.
    click(cx, "orchestrator-pill-project-1");
    assert_eq!(listed(cx), ["12"]);
}
