//! `notes`, `checkpoint restore`, `scripts`, `teleport`, and `fan-out`.
//!
//! Every command acts on the caller's checkout: the workspace of the caller's
//! pane on the caller's daemon, else the workspace the window focuses. A
//! checkout is named by endpoint, repository (Git common directory), and
//! branch, never by workspace ID, which does not survive a daemon restart.
//! Flows that change things for real only open the app's own dialog, already
//! pointed at that checkout; the user confirms them there.
mod flows;
mod notes;
#[cfg(test)]
mod tests;

pub(super) use flows::bring_forward;

use super::{Watches, in_free_window, in_window, not_found, socket, target};
use crate::{
    SheprdWindow,
    control::protocol::{Call, Caller, ErrorCode, Response, work::WorkOp},
};
use gpui::App;
use sheprd_client::protocol::{ClientShellSnapshot, ClientShellWorkspace};

pub(super) fn serve(call: Call<WorkOp>, incoming: socket::Incoming, _: &mut Watches, cx: &mut App) {
    let Call { caller, op } = call;
    let response = match op {
        WorkOp::Notes => notes::list(cx),
        WorkOp::Note => notes::get(&caller, cx),
        WorkOp::SetNote { text } => notes::set(&caller, &text, cx),
        WorkOp::Restore { id } => in_free_window(&caller, cx, |view, window, cx| {
            flows::restore(view, &caller, id, window, cx)
        }),
        WorkOp::Script { kind } => in_window(&caller, cx, |view, _, cx| {
            flows::script(view, &caller, kind, cx)
        }),
        WorkOp::Teleport { host } => in_free_window(&caller, cx, |view, window, cx| {
            flows::teleport(view, &caller, &host, window, cx)
        }),
        WorkOp::FanOut(request) => in_free_window(&caller, cx, |view, window, cx| {
            flows::fan_out(view, &caller, request, window, cx)
        }),
    };
    incoming.respond(response);
}

/// The caller's workspace as one window shows it.
pub(super) struct Found<'a> {
    /// The endpoint showing it, by index into the window's endpoints.
    pub index: usize,
    pub snapshot: &'a ClientShellSnapshot,
    pub workspace: &'a ClientShellWorkspace,
}

/// Finds the caller's workspace in `view`: on the caller's own daemon only
/// when it names one, else on any endpoint showing a workspace of that ID.
pub(super) fn find<'a>(view: &'a SheprdWindow, caller: &Caller) -> Result<Found<'a>, Response> {
    let target = target(caller);
    let (index, id) = target
        .strictness()
        .iter()
        .find_map(|strict| view.control_target(&target, *strict))
        .ok_or_else(|| match &caller.workspace_id {
            Some(id) => Response::error(
                ErrorCode::WorkspaceNotFound,
                format!("No window shows workspace {id}"),
            ),
            None => not_found("The window shows no workspace"),
        })?;
    let live = if index == view.selected_endpoint {
        &view.live
    } else {
        &view.endpoints[index].live
    };
    let snapshot = live
        .snapshot
        .as_deref()
        .ok_or_else(|| not_found("The window has no state from that host yet"))?;
    let workspace = snapshot
        .workspaces
        .iter()
        .find(|workspace| workspace.workspace_id == id)
        .ok_or_else(|| not_found(format!("No window shows workspace {id}")))?;
    Ok(Found {
        index,
        snapshot,
        workspace,
    })
}

/// The caller's workspace ID, when it is on the window's selected, connected
/// host: dialogs act through that host's connection alone.
pub(super) fn selected_workspace(view: &SheprdWindow, caller: &Caller) -> Result<String, Response> {
    let found = find(view, caller)?;
    if found.index != view.selected_endpoint {
        return Err(Response::error(
            ErrorCode::InvalidRequest,
            format!(
                "Workspace {} is on host {}, which the window is not showing; select it first",
                found.workspace.workspace_id, view.endpoints[found.index].label
            ),
        ));
    }
    if !view.live.status.is_connected() {
        return Err(Response::error(
            ErrorCode::Busy,
            "Sheprd is not connected to that host right now",
        ));
    }
    Ok(found.workspace.workspace_id.clone())
}

/// The repository a workspace's checkout belongs to, for messages.
pub(super) fn repository(workspace: &ClientShellWorkspace) -> &str {
    workspace
        .worktree
        .as_ref()
        .map_or(workspace.label.as_str(), |tree| tree.label.as_str())
}
