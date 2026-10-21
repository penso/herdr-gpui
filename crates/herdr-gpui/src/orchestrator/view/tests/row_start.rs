//! An issue row keeps Start agent while a page is open beside the list.
use super::*;
use crate::orchestrator::{
    Access, Request, Timing,
    view::{Look, OrchestratorView, rows::Tab},
};
use gpui::{Modifiers, TestAppContext};
use std::sync::Arc;

#[gpui::test]
fn start_agent_stays_on_a_row_beside_an_open_page(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let request = Request {
        target: herdr_client::ConnectTarget::Local,
        checkout: dir.path().to_string_lossy().into_owned(),
        workspace_id: "w1".into(),
        token: None,
        data_root: Some(dir.path().join("data")),
        timing: Timing::default(),
    };
    let config = crate::config::Config::default();
    let look = Look {
        theme: crate::config::Theme::default(),
        ui: config.ui.clone(),
        mono: config.terminal.clone(),
    };
    let (view, cx) = cx.add_window_view(|_, cx| OrchestratorView::with_repo(request, look, cx));
    cx.simulate_resize(gpui::size(gpui::px(1100.), gpui::px(600.)));
    let issue = item(Provider::Github, "377", "Icons", "2026-10-04T00:00:00Z");
    let key = issue.key.canonical();
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            let repo = view.repo_mut();
            repo.snapshot.items = Arc::new(vec![issue]);
            repo.snapshot.access = Some(Access::ReadWrite);
            view.tab = Tab::Issues;
            view.refresh_rows();
            cx.notify();
        })
    });
    cx.run_until_parked();
    assert!(cx.debug_bounds("orchestrator-start").is_some());
    let row: &'static str = Box::leak(format!("orchestrator-row-{key}").into_boxed_str());
    let at = cx.debug_bounds(row).unwrap().left_center() + gpui::point(gpui::px(8.), gpui::px(0.));
    cx.simulate_click(at, Modifiers::none());
    cx.run_until_parked();
    assert!(cx.debug_bounds("orchestrator-pane").is_some());
    let list = cx.debug_bounds("orchestrator-list").unwrap();
    let start = cx
        .debug_bounds("orchestrator-start")
        .unwrap_or_else(|| panic!("Start agent beside the page"));
    assert!(list.contains(&start.center()), "{start:?} in {list:?}");
    cx.simulate_click(start.center(), Modifiers::none());
    cx.run_until_parked();
    assert!(cx.debug_bounds("orchestrator-overlay-card").is_some());
}
