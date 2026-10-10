//! The dispatch dialog fits short windows and large text by scrolling.
use super::*;
use crate::orchestrator::{
    Request, Timing,
    view::{Look, OrchestratorView},
};
use gpui::{TestAppContext, px, size};
use std::sync::Arc;

#[gpui::test]
fn the_dispatch_dialog_stays_within_a_short_window(cx: &mut TestAppContext) {
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
    let mut ui = config.ui.clone();
    ui.size = 48.;
    let look = Look {
        theme: crate::config::Theme::default(),
        ui,
        mono: config.terminal.clone(),
    };
    let (view, cx) = cx.add_window_view(|_, cx| OrchestratorView::new(request, look, cx));
    cx.simulate_resize(size(px(640.), px(400.)));
    let issue = item(Provider::Github, "377", "Icons", "2026-10-04T00:00:00Z");
    let key = issue.key.canonical();
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.snapshot.items = Arc::new(vec![issue]);
            view.refresh_rows();
            view.open_dispatch(key, window, cx);
        })
    });
    cx.run_until_parked();
    let card = cx
        .debug_bounds("orchestrator-overlay-card")
        .unwrap_or_else(|| panic!("the dialog's card"));
    assert!(
        card.top() >= px(0.) && card.bottom() <= px(400.),
        "{card:?}"
    );
}
