#![allow(clippy::unwrap_used)]
use super::*;
use crate::sidebar::layout_tests::{REPO_KEY, fixture_window, snapshot};

mod dialogs;
mod identity;
mod list_budget;
mod muting;

/// A caller in workspace `workspace` of the fixture window's daemon.
fn caller(workspace: Option<&str>) -> Caller {
    Caller {
        pane_id: None,
        workspace_id: workspace.map(str::to_owned),
        // No daemon: one the window does not show would refuse the request.
        daemon_socket: None,
        window: None,
    }
}

/// The fixture window, connected, with seven workspaces: w3 is a main
/// checkout of [`REPO_KEY`] on `develop`, w4 and w5 linked worktrees of it,
/// w0 a workspace outside Git.
fn connected(view: &mut SheprdWindow) {
    view.live.status = crate::state::ConnectionStatus::Connected;
    let snapshot_ref = std::sync::Arc::make_mut(view.live.snapshot.as_mut().unwrap());
    snapshot_ref.workspaces = snapshot(7).workspaces;
}

fn error_code(response: &Response) -> Option<ErrorCode> {
    match response {
        Response::Error { code, .. } => Some(*code),
        _ => None,
    }
}
