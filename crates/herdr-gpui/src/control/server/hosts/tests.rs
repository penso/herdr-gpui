#![allow(clippy::unwrap_used)]
use super::*;
use crate::sidebar::layout_tests::fixture_window;
use gpui::{Entity, TestAppContext, VisualTestContext};

#[cfg(feature = "cloud")]
mod cloud_setup;
mod info;
mod ports;
mod pr;
mod remote_files;
mod security;
mod security_open;
mod strict;
mod wsl_forwards;

fn caller(workspace: Option<&str>) -> Caller {
    Caller {
        workspace_id: workspace.map(str::to_owned),
        ..Caller::default()
    }
}

/// The layout fixture, with workspace w0 focused as a daemon reports it.
fn window(cx: &mut TestAppContext) -> (Entity<HerdrWindow>, &mut VisualTestContext) {
    cx.add_window_view(|window, cx| {
        let mut view = fixture_window(window, cx);
        if let Some(snapshot) = &view.live.snapshot {
            let mut snapshot = (**snapshot).clone();
            snapshot.focused_workspace_id = Some("w0".into());
            view.live.snapshot = Some(std::sync::Arc::new(snapshot));
        }
        view
    })
}

fn error_code(response: &Response) -> Option<ErrorCode> {
    match response {
        Response::Error { code, .. } => Some(*code),
        _ => None,
    }
}

#[gpui::test]
fn a_request_is_about_the_callers_workspace_or_the_focused_one(cx: &mut TestAppContext) {
    let (view, cx) = window(cx);
    cx.update(|_, cx| {
        let view = view.read(cx);
        let own = Place::of(view, &caller(Some("w3"))).unwrap();
        assert_eq!(
            (own.index, own.endpoint.as_str(), own.workspace.as_str()),
            (0, "local", "w3")
        );
        assert!(own.selected(view));
        assert!(!own.focused(view));
        assert_eq!(
            own.workspace(view).unwrap().branch.as_deref(),
            Some("develop")
        );
        let focused = Place::of(view, &caller(None)).unwrap();
        assert_eq!(focused.workspace, "w0");
        assert!(focused.focused(view));
        let missing = Place::of(view, &caller(Some("w_gone"))).unwrap_err();
        assert_eq!(error_code(&missing), Some(ErrorCode::WorkspaceNotFound));
    });
}
