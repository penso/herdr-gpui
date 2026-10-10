#![allow(clippy::unwrap_used)]

use super::{Dialog, OrchestratorView};
use crate::orchestrator::{Request, Timing, view::Look};
use crate::{search_input::SearchInput, teleport::AgentKind};
use gpui::{AppContext, TestAppContext};
use std::sync::Arc;

#[gpui::test]
fn destination_inventory_reconciles_selection_and_ignores_other_hosts(cx: &mut TestAppContext) {
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
        ui: config.ui,
        mono: config.terminal,
    };
    let (view, cx) = cx.add_window_view(|_, cx| OrchestratorView::new(request, look, cx));
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            let field = cx.new(SearchInput::new);
            view.dialog = Some(Dialog {
                item: "issue".into(),
                profile: None,
                kind: Some(AgentKind::Claude),
                branch: field.clone(),
                extra: field.clone(),
                model: field,
                host: Some("remote".into()),
                problem: None,
                review: false,
            });
            view.select_agent_host(cx);
            assert!(view.dialog.as_ref().unwrap().kind.is_none());
            view.snapshot.installed_endpoint = None;
            view.snapshot.installed = Some(Arc::new(vec![AgentKind::Claude]));
            view.reconcile_agents();
            assert!(view.dialog.as_ref().unwrap().kind.is_none());
            view.snapshot.installed_endpoint = Some("remote".into());
            view.reconcile_agents();
            assert_eq!(view.dialog.as_ref().unwrap().kind, Some(AgentKind::Claude));
            view.snapshot.installed = Some(Arc::new(vec![]));
            view.reconcile_agents();
            assert!(view.dialog.as_ref().unwrap().kind.is_none());
        })
    });
}
