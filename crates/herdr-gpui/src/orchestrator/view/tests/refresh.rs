//! Refresh spins and ignores presses while a sync runs.
use super::*;
use crate::orchestrator::{
    Request, SourceStatus, SyncState, Timing,
    view::{Look, OrchestratorView},
};
use gpui::{Modifiers, TestAppContext, VisualTestContext};

fn busy(cx: &mut VisualTestContext) -> bool {
    cx.run_until_parked();
    let idle = cx.debug_bounds("orchestrator-refresh").is_some();
    let busy = cx.debug_bounds("orchestrator-refresh-busy").is_some();
    assert_ne!(idle, busy);
    busy
}

#[gpui::test]
fn refresh_spins_until_the_sync_it_asked_for_ends(cx: &mut TestAppContext) {
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
    let set = |cx: &mut VisualTestContext, state: SyncState| {
        cx.update(|_, cx| {
            view.update(cx, |view, cx| {
                view.repo_mut().snapshot.sources = vec![SourceStatus {
                    key: source(Provider::Github),
                    state,
                }];
                view.refresh_asked = None;
                cx.notify();
            })
        })
    };
    set(cx, SyncState::SignIn);
    assert!(!busy(cx));
    // A press spins the icon at once, before the worker reports its sync.
    let at = cx.debug_bounds("orchestrator-refresh").unwrap().center();
    cx.simulate_click(at, Modifiers::none());
    assert!(busy(cx));
    let asked = cx.update(|_, cx| view.read(cx).refresh_asked);
    assert!(asked.is_some());
    // Another press while it spins asks for nothing more.
    cx.simulate_click(at, Modifiers::none());
    assert_eq!(cx.update(|_, cx| view.read(cx).refresh_asked), asked);
    // The worker's sync keeps it spinning; its end stops it.
    set(cx, SyncState::Syncing { count: None });
    assert!(busy(cx));
    set(cx, SyncState::Capped { count: 3 });
    assert!(!busy(cx));
}
