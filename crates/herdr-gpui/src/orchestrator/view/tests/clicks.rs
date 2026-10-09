//! Clicks in a real window reach the view's handlers.
use super::*;
use crate::orchestrator::{
    Request, Timing,
    view::{Event, Look, OrchestratorView, rows::Tab},
};
use gpui::{Modifiers, TestAppContext};
use std::{cell::RefCell, rc::Rc, sync::Arc};

#[gpui::test]
fn the_run_arrow_asks_to_open_the_run(cx: &mut TestAppContext) {
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
    let (view, cx) = cx.add_window_view(|_, cx| OrchestratorView::new(request, look, cx));
    let events: Rc<RefCell<Vec<Event>>> = Rc::default();
    let seen = events.clone();
    cx.update(|_, cx| {
        cx.subscribe(&view, move |_, event: &Event, _| {
            seen.borrow_mut().push(event.clone())
        })
        .detach();
    });
    let issue = item(Provider::Github, "377", "Icons", "2026-10-04T00:00:00Z");
    let theirs = run("r1", &issue, RunState::Failed, "2026-10-04T01:00:00Z");
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.snapshot.runs = Arc::new(vec![theirs]);
            view.tab = Tab::Runs;
            view.refresh_rows();
            cx.notify();
        })
    });
    cx.run_until_parked();
    let bounds = cx
        .debug_bounds("orchestrator-run-open-1")
        .unwrap_or_else(|| panic!("the arrow of the first run, after its group heading"));
    cx.simulate_click(bounds.center(), Modifiers::none());
    cx.run_until_parked();
    let events = events.borrow();
    assert!(
        matches!(
            events.as_slice(),
            [Event::OpenWorkspace { host: None, workspace_id }] if workspace_id == "w-r1"
        ),
        "{events:?}"
    );
}
