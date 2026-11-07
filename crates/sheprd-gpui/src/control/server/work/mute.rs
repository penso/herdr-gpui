//! `worktree`: the mute of the caller's workspace. Mutes stay in this app;
//! nothing reaches the daemon.
use super::find;
use crate::{
    SheprdWindow,
    control::protocol::{
        Caller, ErrorCode, Response,
        work::{MuteInfo, Reply},
    },
    notification_mutes::{Daemon, Mutes, Scope},
};
use gpui::{App, Context};
use std::time::SystemTime;

pub(super) fn status(view: &SheprdWindow, caller: &Caller, cx: &App) -> Response {
    match find(view, caller) {
        Ok(found) => {
            let endpoint = &view.endpoints[found.index];
            let silenced = Mutes::of(cx)
                .map(|mutes| mutes.silenced(&Daemon::of(endpoint), found.snapshot))
                .unwrap_or_default();
            Response::Work {
                reply: Reply::Mute {
                    mute: MuteInfo {
                        host: endpoint.label.clone(),
                        workspace: found.workspace.label.clone(),
                        muted: silenced.workspace(&found.workspace.workspace_id),
                        host_muted: silenced.host(),
                    },
                },
            }
        }
        Err(response) => response,
    }
}

pub(super) fn set(
    view: &SheprdWindow,
    caller: &Caller,
    muted: bool,
    cx: &mut Context<SheprdWindow>,
) -> Response {
    let found = match find(view, caller) {
        Ok(found) => found,
        Err(response) => return response,
    };
    let daemon = Daemon::of(&view.endpoints[found.index]);
    let id = &found.workspace.workspace_id;
    let changed = Mutes::update(cx, |mutes| {
        mutes.set(
            &daemon,
            found.snapshot,
            Scope::Workspace,
            id,
            muted,
            SystemTime::now(),
        )
    });
    if changed {
        cx.notify();
    }
    let response = status(view, caller, cx);
    match &response {
        Response::Work {
            reply: Reply::Mute { mute },
        } if mute.muted != muted => Response::error(
            ErrorCode::InvalidRequest,
            "This workspace's ID is too long to keep a mute on",
        ),
        _ => response,
    }
}
