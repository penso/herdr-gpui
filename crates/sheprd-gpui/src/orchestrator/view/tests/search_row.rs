//! The Pull requests list's search row: its controls leave a narrow list's
//! field no room, so the field takes its own line and dropdowns move down.
use crate::orchestrator::{
    Request, Timing,
    view::{Look, Menu, OrchestratorView, rows::Tab},
};
use gpui::{Bounds, Pixels, TestAppContext, VisualTestContext};

fn view(
    cx: &mut TestAppContext,
    width: f32,
) -> (gpui::Entity<OrchestratorView>, &mut VisualTestContext) {
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
    cx.simulate_resize(gpui::size(gpui::px(width), gpui::px(600.)));
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.tab = Tab::PullRequests;
            view.refresh_rows();
            cx.notify();
        })
    });
    cx.run_until_parked();
    (view, cx)
}

fn bounds(cx: &mut VisualTestContext, selector: &'static str) -> Bounds<Pixels> {
    cx.debug_bounds(selector)
        .unwrap_or_else(|| panic!("{selector} is not drawn"))
}

#[gpui::test]
fn a_narrow_list_gives_the_search_field_its_own_line(cx: &mut TestAppContext) {
    let (view, cx) = view(cx, 700.);
    let search = bounds(cx, "orchestrator-search");
    let open = bounds(cx, "orchestrator-open");
    assert!(search.size.width > gpui::px(400.));
    assert!(search.size.height > gpui::px(0.));
    assert!(search.bottom() <= open.top());
    // A dropdown opens below the controls, not over them.
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.toggle_menu(Menu::State);
            cx.notify();
        })
    });
    cx.run_until_parked();
    let first = bounds(cx, "orchestrator-state-all");
    assert!(first.top() >= open.bottom());
}

#[gpui::test]
fn a_wide_list_keeps_the_field_beside_its_controls(cx: &mut TestAppContext) {
    let (_, cx) = view(cx, 1400.);
    let search = bounds(cx, "orchestrator-search");
    let open = bounds(cx, "orchestrator-open");
    assert!(search.right() <= open.left());
    assert!(search.top() < open.bottom() && open.top() < search.bottom());
}
