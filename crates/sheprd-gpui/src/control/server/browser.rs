//! Browser tabs, page notes, and the agents waiting for them.
use super::super::{
    Opening, Placed, Placement, Target, main_windows, plain, plain_path,
    protocol::{
        BrowserOpen, Caller, ErrorCode, FeedbackRequest, MAX_WAIT_SECONDS, NotesRequest, NotesTo,
        OpenedIn, Page, Response,
    },
    socket, valid_caller,
};
use crate::browser::{Feedback, FeedbackKey, LocalFile, Location, Scope, WebUrl};
use gpui::App;
use std::path::Path;

fn location(page: &Page) -> Result<Location, Response> {
    match page {
        Page::Url(url) => WebUrl::try_from(url.as_str())
            .map(|url| Location::Web { url })
            .map_err(|error| Response::error(ErrorCode::InvalidUrl, error.to_string())),
        Page::File(path) => {
            let home = crate::config::home().ok();
            LocalFile::new(Path::new(path), home.as_deref())
                .map(|file| Location::Local { file })
                .map_err(|error| Response::error(ErrorCode::InvalidUrl, error.to_string()))
        }
    }
}

pub(super) fn open_browser(request: &BrowserOpen, cx: &mut App) -> Response {
    if !valid_caller(&request.caller) {
        return Response::error(
            ErrorCode::InvalidRequest,
            "Invalid workspace, pane, or daemon",
        );
    }
    let location = match location(&request.page) {
        Ok(location) => location,
        Err(response) => return response,
    };
    if !crate::browser::EMBEDDED {
        let Location::Web { url } = &location else {
            return Response::error(
                ErrorCode::Unsupported,
                "Local pages need browser tabs, which this platform's build does not have",
            );
        };
        cx.open_url(url.as_str());
        return Response::Opened {
            opened_in: OpenedIn::SystemBrowser,
            workspace_id: None,
            placement: Placement::Tab,
        };
    }
    let mut windows = main_windows(cx);
    if windows.is_empty() {
        return Response::error(ErrorCode::NoWindow, "Sheprd has no window open");
    }
    if let Some(id) = request.caller.window {
        windows.retain(|handle| handle.window_id().as_u64() == id);
        if windows.is_empty() {
            return Response::error(ErrorCode::NotFound, format!("Sheprd has no window {id}"));
        }
    }
    let caller = &request.caller;
    let target = Target {
        daemon: caller.daemon_socket.as_deref().map(Path::new),
        workspace: caller.workspace_id.as_deref(),
        pane: caller.pane_id.as_deref(),
    };
    // First the window connected to the caller's own daemon; then, when the
    // socket paths are spelled differently, any window showing its workspace.
    for strict in [true, false] {
        for handle in &windows {
            let opening = Opening {
                location: &location,
                focus: request.focus,
                placement: request.placement,
            };
            let placed = handle.update(cx, |view, window, cx| {
                view.open_requested_browser_tab(&target, strict, &opening, window, cx)
            });
            match placed {
                Ok(Some(Placed::Opened {
                    workspace_id,
                    placement,
                })) => {
                    return Response::Opened {
                        opened_in: OpenedIn::Tab,
                        workspace_id: Some(workspace_id),
                        placement,
                    };
                }
                Ok(Some(Placed::Full)) => {
                    return Response::error(ErrorCode::TabLimit, "Too many browser tabs are open");
                }
                Ok(None) | Err(_) => {}
            }
        }
    }
    Response::error(
        ErrorCode::WorkspaceNotFound,
        match &caller.workspace_id {
            Some(id) => format!("No Sheprd window shows workspace {id}"),
            None => "No Sheprd window shows a workspace".into(),
        },
    )
}

/// Reloads every page the calling pane opened, in every window.
pub(super) fn reload(caller: &Caller, cx: &mut App) -> Response {
    let Some(pane) = caller.pane_id.as_deref().filter(|pane| plain(pane, 256)) else {
        return Response::error(
            ErrorCode::InvalidRequest,
            "browser reload runs in the Herdr pane that opened the page",
        );
    };
    let tabs: Vec<_> = cx
        .try_global::<crate::browser::Store>()
        .map(|store| store.opened_by(pane).map(|tab| tab.id).collect())
        .unwrap_or_default();
    for handle in main_windows(cx) {
        let _ = handle.update(cx, |view, _, _| view.reload_browser_tabs(&tabs));
    }
    Response::Reloaded { tabs: tabs.len() }
}

/// Notes are pasted into an agent's prompt, so they keep their line breaks
/// and lose every other control character: none can end the paste early.
fn pastable(text: &str) -> String {
    text.chars()
        .filter(|c| matches!(c, '\n' | '\t') || !crate::notifications::unsafe_char(*c))
        .collect()
}

/// Hands notes from outside a window to the agent in the caller's pane: the
/// window showing that pane delivers them as it does page notes, and with
/// none they are kept for `browser feedback`.
pub(super) fn notes(request: &NotesRequest, cx: &mut App) -> Response {
    let caller = &request.caller;
    let Some(pane) = caller.pane_id.as_deref().filter(|_| valid_caller(caller)) else {
        return Response::error(
            ErrorCode::InvalidRequest,
            "Notes are sent from the Herdr pane of the agent they are for",
        );
    };
    let Some(daemon) = caller.daemon_socket.as_deref() else {
        return Response::error(
            ErrorCode::InvalidRequest,
            "Notes require the caller's daemon socket",
        );
    };
    let recipient = FeedbackKey {
        scope: Scope::local(Path::new(daemon)),
        pane_id: pane.to_owned(),
    };
    let text = pastable(&request.text);
    if text.trim().is_empty() {
        return Response::error(ErrorCode::InvalidRequest, "There are no notes to send");
    }
    // One take hands out at most this much, so a larger batch could never
    // be fetched.
    if crate::browser::json_size(&text) > crate::browser::TAKE_BUDGET {
        return Response::error(
            ErrorCode::InvalidRequest,
            "These notes are too long to send",
        );
    }
    let target = Target {
        daemon: caller.daemon_socket.as_deref().map(Path::new),
        workspace: None,
        pane: Some(pane),
    };
    for handle in main_windows(cx) {
        let taken = handle.update(cx, |view, _, cx| {
            view.deliver_requested_notes(&target, &text, cx)
        });
        if let Ok(Some(to)) = taken {
            return Response::NotesSent { to };
        }
    }
    let feedback = cx.default_global::<Feedback>();
    let to = if feedback.is_waiting(&recipient) {
        NotesTo::Agent
    } else {
        NotesTo::Kept
    };
    feedback.keep(
        crate::browser::Batch {
            target: recipient,
            text,
        },
        None,
    );
    Response::NotesSent { to }
}

/// An agent waiting in `browser feedback --wait`.
pub(super) struct Waiter {
    target: FeedbackKey,
    until: std::time::Instant,
    incoming: socket::Incoming,
}

/// At most this many agents wait at once; the socket bounds connections too.
const MAX_WAITERS: usize = 8;

pub(super) fn feedback(
    request: &FeedbackRequest,
    incoming: socket::Incoming,
    waiters: &mut Vec<Waiter>,
    cx: &mut App,
) {
    if !plain(&request.pane_id, 256) {
        incoming.respond(Response::error(ErrorCode::InvalidRequest, "Invalid pane"));
        return;
    }
    let Some(daemon) = request
        .daemon_socket
        .as_deref()
        .filter(|path| plain_path(path))
    else {
        incoming.respond(Response::error(
            ErrorCode::InvalidRequest,
            "Feedback requires the caller's daemon socket",
        ));
        return;
    };
    let target = FeedbackKey {
        scope: Scope::local(Path::new(daemon)),
        pane_id: request.pane_id.clone(),
    };
    let kept = take_feedback(&target, cx);
    if kept.is_some() || request.wait_seconds == 0 {
        respond_feedback(incoming, target, kept, cx);
        return;
    }
    if waiters.len() >= MAX_WAITERS {
        incoming.respond(Response::error(
            ErrorCode::Busy,
            "Too many agents are waiting for notes",
        ));
        return;
    }
    // A newer wait from the same pane replaces the older one.
    if let Some(index) = waiters.iter().position(|waiter| waiter.target == target) {
        waiters
            .remove(index)
            .incoming
            .respond(Response::Feedback { text: None });
    }
    waiters.push(Waiter {
        target,
        until: std::time::Instant::now()
            + std::time::Duration::from_secs(request.wait_seconds.min(MAX_WAIT_SECONDS)),
        incoming,
    });
}

/// Takes what is kept for `target`. A tab shows its batch as kept until an
/// agent collects it, so the windows redraw once one does.
fn take_feedback(target: &FeedbackKey, cx: &mut App) -> Option<String> {
    let text = cx.default_global::<Feedback>().take(target);
    if text.is_some() {
        cx.refresh_windows();
    }
    text
}

fn respond_feedback(
    incoming: socket::Incoming,
    target: FeedbackKey,
    text: Option<String>,
    cx: &mut App,
) {
    if let Err(Response::Feedback { text: Some(text) }) =
        incoming.try_respond(Response::Feedback { text })
    {
        // Back first in line: it was the oldest when it was taken.
        cx.default_global::<Feedback>()
            .keep_again(crate::browser::Batch { target, text });
    }
}

/// Hands sent notes to waiting agents and releases the ones whose wait ran
/// out, then tells the windows who is still waiting.
pub(super) fn serve_waiters(waiters: &mut Vec<Waiter>, cx: &mut App) {
    let now = std::time::Instant::now();
    let mut still = Vec::with_capacity(waiters.len());
    for waiter in waiters.drain(..) {
        if !waiter.incoming.is_live() {
            continue;
        }
        let has = cx
            .try_global::<Feedback>()
            .is_some_and(|feedback| feedback.has(&waiter.target));
        if has {
            let text = take_feedback(&waiter.target, cx);
            respond_feedback(waiter.incoming, waiter.target, text, cx);
        } else if now >= waiter.until {
            waiter.incoming.respond(Response::Feedback { text: None });
        } else {
            still.push(waiter);
        }
    }
    *waiters = still;
    let panes: Vec<FeedbackKey> = waiters.iter().map(|waiter| waiter.target.clone()).collect();
    let current = cx.try_global::<Feedback>().map(Feedback::waiting);
    if current.is_none_or(|current| current != panes.as_slice()) {
        cx.default_global::<Feedback>().set_waiting(panes);
    }
}

#[cfg(test)]
mod tests;
