//! Clicks in a real window reach the view's handlers.
use super::*;
use crate::orchestrator::{
    Request, Timing,
    view::{Look, OrchestratorView, rows::Tab},
};
use gpui::{Modifiers, TestAppContext};
use std::sync::Arc;

#[gpui::test]
fn a_click_opens_the_item_beside_the_list(cx: &mut TestAppContext) {
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
            view.repo_mut().snapshot.items = Arc::new(vec![issue]);
            view.tab = Tab::Issues;
            view.refresh_rows();
            cx.notify();
        })
    });
    cx.run_until_parked();
    let row: &'static str = Box::leak(format!("orchestrator-row-{key}").into_boxed_str());
    let list = cx.debug_bounds("orchestrator-list").unwrap();
    let at = cx.debug_bounds(row).unwrap().center();
    assert!(cx.debug_bounds("orchestrator-pane").is_none());
    // One click opens the item in a pane beside the list.
    cx.simulate_click(at, Modifiers::none());
    cx.run_until_parked();
    let pane = cx.debug_bounds("orchestrator-pane").unwrap();
    let narrowed = cx.debug_bounds("orchestrator-list").unwrap();
    assert!(narrowed.size.width < list.size.width);
    assert!(pane.left() >= narrowed.right());
    // Its close button gives the list its width back.
    let close = cx.debug_bounds("orchestrator-detail-close").unwrap();
    cx.simulate_click(close.center(), Modifiers::none());
    cx.run_until_parked();
    assert!(cx.debug_bounds("orchestrator-pane").is_none());
    assert_eq!(
        cx.debug_bounds("orchestrator-list").unwrap().size,
        list.size
    );
    // Switching to pull requests leaves the issue behind.
    cx.simulate_click(at, Modifiers::none());
    cx.run_until_parked();
    assert!(cx.debug_bounds("orchestrator-pane").is_some());
    cx.update(|_, cx| view.update(cx, |view, cx| view.show_list(Tab::PullRequests, cx)));
    cx.run_until_parked();
    assert!(cx.debug_bounds("orchestrator-pane").is_none());
    cx.update(|_, cx| view.update(cx, |view, cx| view.show_list(Tab::Issues, cx)));
    cx.run_until_parked();
    assert!(cx.debug_bounds("orchestrator-pane").is_none());
}
