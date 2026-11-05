//! Tabs opened in the caller's workspace: code, the editor, Devices, the
//! orchestrator, and the review. Each feature opens its tab in the focused
//! workspace, so the window first switches to the caller's, and the tab
//! opens once a snapshot says the switch landed.
use super::{busy, closed};
use crate::{
    SheprdWindow,
    browser::{Location, OrchestratorRepo, ReviewCheckout, Store},
    control::{
        protocol::{
            Caller, ErrorCode, Response,
            tabs::{ARRIVE_WAIT_SECONDS, Reply},
        },
        server::{Watches, socket, target, window_for},
    },
    editor::EditorTarget,
};
use gpui::{App, Context, Window, WindowHandle};
use std::{path::Path, time::Duration};

/// The longest path a request may name.
const MAX_PATH: usize = 4096;

/// A local file a request names: absolute, bounded, and printable. Whether
/// it exists is for the window's own reading to find, off the UI thread.
fn editor_target(path: &str, line: Option<u32>) -> Result<EditorTarget, Response> {
    let valid = !path.is_empty()
        && path.len() <= MAX_PATH
        && !path.chars().any(char::is_control)
        && Path::new(path).is_absolute();
    if !valid {
        return Err(Response::error(
            ErrorCode::InvalidRequest,
            "The file must be an absolute path without control characters",
        ));
    }
    if line == Some(0) {
        return Err(Response::error(
            ErrorCode::InvalidRequest,
            "Lines are counted from 1",
        ));
    }
    Ok(EditorTarget {
        path: path.into(),
        line,
    })
}

/// The window, endpoint, and workspace a request's tab goes to.
struct Destination {
    handle: WindowHandle<SheprdWindow>,
    endpoint: usize,
    workspace: String,
}

fn destination(caller: &Caller, cx: &mut App) -> Result<Destination, Response> {
    let handle = window_for(caller, cx)?;
    let target = target(caller);
    let found = handle
        .read_with(cx, |view, _| {
            target
                .strictness()
                .iter()
                .find_map(|strict| view.control_target(&target, *strict))
        })
        .map_err(|_| closed())?;
    let (endpoint, workspace) = found.ok_or_else(|| {
        Response::error(
            ErrorCode::WorkspaceNotFound,
            match &caller.workspace_id {
                Some(id) => format!("No Sheprd window shows workspace {id}"),
                None => "The window shows no workspace yet".to_owned(),
            },
        )
    })?;
    Ok(Destination {
        handle,
        endpoint,
        workspace,
    })
}

/// Switches the window to the caller's workspace, then, once the switch
/// landed and `ready` holds, runs `act` there and answers with what it says.
fn open_there(
    caller: &Caller,
    incoming: socket::Incoming,
    watches: &mut Watches,
    cx: &mut App,
    ready: impl Fn(&SheprdWindow) -> bool + 'static,
    act: impl FnOnce(&mut SheprdWindow, &str, &mut Window, &mut Context<SheprdWindow>) -> Response
    + 'static,
) {
    let Destination {
        handle,
        endpoint,
        workspace,
    } = match destination(caller, cx) {
        Ok(destination) => destination,
        Err(response) => return incoming.respond(response),
    };
    let started = handle
        .update(cx, |view, _, cx| {
            if view.menu.page.is_some() {
                return Err(busy());
            }
            view.go_to_workspace(endpoint, &workspace, cx);
            Ok(())
        })
        .unwrap_or_else(|_| Err(closed()));
    if let Err(response) = started {
        return incoming.respond(response);
    }
    let expired = Response::error(
        ErrorCode::Timeout,
        format!("Workspace {workspace} was not ready in the window in time"),
    );
    let mut act = Some(act);
    watches.add(
        incoming,
        Duration::from_secs(ARRIVE_WAIT_SECONDS),
        move |cx| {
            let arrived = handle.read_with(cx, |view, _| {
                view.shows_workspace(endpoint, &workspace) && ready(view)
            });
            match arrived {
                Err(_) => return Some(closed()),
                Ok(false) => return None,
                Ok(true) => {}
            }
            let act = act.take()?;
            Some(
                handle
                    .update(cx, |view, window, cx| {
                        if view.menu.page.is_some() {
                            return busy();
                        }
                        act(view, &workspace, window, cx)
                    })
                    .unwrap_or_else(|_| closed()),
            )
        },
        expired,
        cx,
    );
}

/// The tab of the window's focused workspace that `shows` picks, as the
/// answer to opening it; the feature already told the user why when none.
fn opened(
    view: &SheprdWindow,
    workspace: &str,
    what: &str,
    cx: &App,
    shows: impl Fn(&Location) -> bool,
) -> Response {
    let scope = crate::browser::scope(&view.endpoints[view.selected_endpoint]);
    let id = cx.try_global::<Store>().and_then(|store| {
        store
            .in_workspace(&scope, workspace)
            .filter(|tab| !tab.place.is_editor())
            .filter(|tab| tab.location.as_ref().is_some_and(&shows))
            .last()
            .map(|tab| tab.id)
    });
    match id {
        Some(id) => Response::Tab {
            reply: Reply::Opened {
                id: id.into(),
                workspace: workspace.to_owned(),
            },
        },
        None => Response::error(
            ErrorCode::Failed,
            format!("Could not open {what}; the window says why"),
        ),
    }
}

pub(super) fn code(
    caller: &Caller,
    path: &str,
    line: Option<u32>,
    incoming: socket::Incoming,
    watches: &mut Watches,
    cx: &mut App,
) {
    let target = match editor_target(path, line) {
        Ok(target) => target,
        Err(response) => return incoming.respond(response),
    };
    let path = path.to_owned();
    open_there(
        caller,
        incoming,
        watches,
        cx,
        |_| true,
        move |view, workspace, window, cx| {
            view.open_code_view(&target, window, cx);
            opened(
                view,
                workspace,
                &path,
                cx,
                |location| matches!(location, Location::Code { file } if file.path == path),
            )
        },
    );
}

/// Opens the file in the user's editor, in a pane split beside the
/// caller's, when the window shows it, else beside the focused pane. The
/// editor reports its failures in the window.
pub(super) fn edit(
    caller: &Caller,
    path: &str,
    line: Option<u32>,
    incoming: socket::Incoming,
    watches: &mut Watches,
    cx: &mut App,
) {
    let target = match editor_target(path, line) {
        Ok(target) => target,
        Err(response) => return incoming.respond(response),
    };
    let pane = caller.pane_id.clone();
    let path = path.to_owned();
    open_there(
        caller,
        incoming,
        watches,
        cx,
        |_| true,
        move |view, _, _, cx| {
            let beside = pane.as_deref().filter(|pane| {
                view.live.snapshot.as_ref().is_some_and(|snapshot| {
                    snapshot.panes.iter().any(|shown| shown.pane_id == *pane)
                })
            });
            view.open_in_editor(&target, beside, cx);
            Response::done(format!(
                "Opening {path} in your editor; a problem would show in the window"
            ))
        },
    );
}

pub(super) fn devices(
    caller: &Caller,
    incoming: socket::Incoming,
    watches: &mut Watches,
    cx: &mut App,
) {
    open_there(
        caller,
        incoming,
        watches,
        cx,
        |_| true,
        |view, workspace, window, cx| {
            view.open_devices_overview(window, cx);
            opened(view, workspace, "the Devices overview", cx, |location| {
                matches!(location, Location::Devices)
            })
        },
    );
}

/// The virtual Mac called `name`, in a tab of the caller's workspace.
pub(super) fn machine(
    caller: &Caller,
    name: String,
    incoming: socket::Incoming,
    watches: &mut Watches,
    cx: &mut App,
) {
    let Ok(name) = crate::machine::Name::try_from(name) else {
        return incoming.respond(Response::error(
            ErrorCode::InvalidRequest,
            "A machine name takes letters, digits, '-', and '_', and starts with a letter or digit",
        ));
    };
    open_there(
        caller,
        incoming,
        watches,
        cx,
        |_| true,
        move |view, workspace, window, cx| {
            view.open_machine(name.clone(), window, cx);
            opened(
                view,
                workspace,
                "the machine",
                cx,
                |location| matches!(location, Location::Machine { name: open } if *open == name),
            )
        },
    );
}

/// The workspace's issues in the activity bar's Issues section, listing the
/// repository of `repo`, or of the workspace's focused pane.
pub(super) fn orchestrator(
    caller: &Caller,
    repo: Option<String>,
    incoming: socket::Incoming,
    watches: &mut Watches,
    cx: &mut App,
) {
    if let Some(repo) = &repo
        && OrchestratorRepo::new(repo.clone()).is_err()
    {
        return incoming.respond(Response::error(
            ErrorCode::InvalidRequest,
            "The repository must be an absolute path without control characters",
        ));
    }
    open_there(
        caller,
        incoming,
        watches,
        cx,
        |_| true,
        move |view, workspace, window, cx| {
            view.open_orchestrator_in(workspace.to_owned(), repo, window, cx);
            if view.activity.panel == crate::sidebar::ActivityPanel::Issues
                && view.activity_section().is_some()
            {
                Response::done(format!("Showing the issues of workspace {workspace}"))
            } else {
                Response::error(
                    ErrorCode::Unsupported,
                    "Issues show in the activity bar, which is turned off, or this host cannot list them",
                )
            }
        },
    );
}

/// The review of the workspace's checkout, once the window's Git worker
/// follows that checkout rather than the one shown before the switch.
pub(super) fn review(
    caller: &Caller,
    incoming: socket::Incoming,
    watches: &mut Watches,
    cx: &mut App,
) {
    open_there(
        caller,
        incoming,
        watches,
        cx,
        |view| {
            let input = view.git_input();
            input.is_none() || view.git.tracked() == input.as_ref()
        },
        |view, workspace, window, cx| {
            let Some(checkout) = view
                .git_input()
                .and_then(|input| ReviewCheckout::try_from(&input).ok())
            else {
                return Response::error(
                    ErrorCode::Failed,
                    format!("Workspace {workspace} has no local Git checkout to review"),
                );
            };
            view.open_review(window, cx);
            opened(
                view,
                workspace,
                "the review",
                cx,
                |location| matches!(location, Location::Review { checkout: open } if *open == checkout),
            )
        },
    );
}

#[cfg(test)]
mod tests;

pub(super) fn px0(
    caller: &Caller,
    incoming: socket::Incoming,
    watches: &mut Watches,
    cx: &mut App,
) {
    open_there(
        caller,
        incoming,
        watches,
        cx,
        |_| true,
        |view, workspace, window, cx| {
            if !crate::browser::EMBEDDED || view.px0_root().is_none() {
                return Response::error(
                    ErrorCode::Unsupported,
                    "px0 needs a local workspace and embedded pages (macOS or Windows)",
                );
            }
            match view.open_px0(None, window, cx) {
                Some(id) => Response::Tab {
                    reply: Reply::Opened {
                        id: id.into(),
                        workspace: workspace.to_owned(),
                    },
                },
                None => Response::error(ErrorCode::Failed, "Could not open the px0 tab"),
            }
        },
    );
}
