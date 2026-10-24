//! The app's side of the control socket: requests are answered on the UI
//! thread, a few at a time, from the windows and the app's globals. Most are
//! answered at once; a request that waits for something (notes, an event, a
//! page) is parked as a [`Watch`] and checked on every tick.
mod app;
mod browser;
mod chat;
mod cloud;
mod events;
mod hosts;
mod links;
mod remote;
mod settings;
mod tabs;
mod work;

use super::{
    Target, main_windows, plain,
    protocol::{Caller, ErrorCode, MAX_WAIT_SECONDS, Request, Response},
    socket, socket_path, valid_caller,
};
use crate::{
    HerdrWindow,
    browser::{Feedback, FeedbackKey, Scope},
};
use browser::{feedback, notes, open_browser, reload, serve_waiters};
use gpui::{App, Context, Window, WindowHandle};
use std::{
    path::Path,
    time::{Duration, Instant},
};

/// Requests parked at once, besides feedback waiters; the socket's own
/// connection bound keeps the real number lower.
const MAX_WATCHES: usize = 16;

/// Looks for a parked request's answer; `None` keeps it waiting.
type Check = Box<dyn FnMut(&mut App) -> Option<Response>>;

/// A request answered once `check` has an answer, or with `expired` when
/// `until` passes or the caller hangs up.
pub(super) struct Watch {
    incoming: socket::Incoming,
    until: Instant,
    check: Check,
    expired: Response,
}

#[derive(Default)]
pub(super) struct Watches(Vec<Watch>);

impl Watches {
    /// Parks `incoming` for up to `wait`. Checks once at once, so an answer
    /// already there is never delayed a tick.
    pub(super) fn add(
        &mut self,
        incoming: socket::Incoming,
        wait: Duration,
        mut check: impl FnMut(&mut App) -> Option<Response> + 'static,
        expired: Response,
        cx: &mut App,
    ) {
        if let Some(response) = check(cx) {
            incoming.respond(response);
            return;
        }
        if self.0.len() >= MAX_WATCHES {
            incoming.respond(Response::error(
                ErrorCode::Busy,
                "Too many requests are waiting",
            ));
            return;
        }
        self.0.push(Watch {
            incoming,
            until: deadline(wait, Instant::now()),
            check: Box::new(check),
            expired,
        });
    }

    /// Whether another request can park. Checked before a request that may
    /// park acts at all, so a full list never refuses after a side effect.
    fn full(&self) -> bool {
        self.0.len() >= MAX_WATCHES
    }

    fn serve(&mut self, cx: &mut App) {
        let now = Instant::now();
        let mut still = Vec::with_capacity(self.0.len());
        for mut watch in self.0.drain(..) {
            if !watch.incoming.is_live() {
                continue;
            }
            if let Some(response) = (watch.check)(cx) {
                watch.incoming.respond(response);
            } else if now >= watch.until {
                watch.incoming.respond(watch.expired);
            } else {
                still.push(watch);
            }
        }
        self.0 = still;
    }
}

/// When a parked request expires. Clamped: a request's wait is any number a
/// local process sent, and `Instant` addition panics on overflow.
fn deadline(wait: Duration, now: Instant) -> Instant {
    now + wait.min(Duration::from_secs(MAX_WAIT_SECONDS))
}

/// Everything the serving loop keeps between ticks.
#[derive(Default)]
struct State {
    waiters: Vec<browser::Waiter>,
    watches: Watches,
}

fn not_found(message: impl Into<String>) -> Response {
    Response::error(ErrorCode::NotFound, message)
}

/// The window a request acts in: the one `--window` named; else the one
/// showing the caller's workspace (on the caller's own daemon when it names
/// one); else, for a caller outside any workspace, the frontmost.
pub(super) fn window_for(
    caller: &Caller,
    cx: &mut App,
) -> Result<WindowHandle<HerdrWindow>, Response> {
    if !valid_caller(caller) {
        return Err(Response::error(
            ErrorCode::InvalidRequest,
            "Invalid workspace, pane, or daemon",
        ));
    }
    let windows = main_windows(cx);
    if let Some(id) = caller.window {
        return windows
            .into_iter()
            .find(|handle| handle.window_id().as_u64() == id)
            .ok_or_else(|| not_found(format!("Herdr UI has no window {id}")));
    }
    if let Some(workspace) = &caller.workspace_id {
        let target = target(caller);
        for strict in target.strictness() {
            for handle in &windows {
                let shows = handle
                    .read_with(cx, |view, _| {
                        view.control_target(&target, *strict).is_some()
                    })
                    .unwrap_or(false);
                if shows {
                    return Ok(*handle);
                }
            }
        }
        // A window showing the caller's own daemon but listing no workspace,
        // such as one detached from it, is still the caller's: `window
        // reconnect` must reach it. Only when that daemon is the shown host:
        // window-wide commands act on the shown host.
        if let Some(daemon) = target.daemon {
            let own = windows.iter().find(|handle| {
                handle
                    .read_with(cx, |view, _| view.shows_daemon(daemon))
                    .unwrap_or(false)
            });
            if let Some(handle) = own {
                return Ok(*handle);
            }
        }
        // The caller's workspace is in no window, perhaps of a session the
        // app does not show: acting on the frontmost window would change
        // another session's.
        return Err(Response::error(
            ErrorCode::WorkspaceNotFound,
            format!("No Herdr UI window shows workspace {workspace}"),
        ));
    }
    windows
        .into_iter()
        .next()
        .ok_or_else(|| Response::error(ErrorCode::NoWindow, "Herdr UI has no window open"))
}

pub(super) fn target(caller: &Caller) -> Target<'_> {
    Target {
        daemon: caller.daemon_socket.as_deref().map(Path::new),
        workspace: caller.workspace_id.as_deref(),
        pane: caller.pane_id.as_deref(),
    }
}

/// Runs `act` in the request's window. A window holding a dialog takes no
/// commands, so a request that would act through its menus is refused
/// rather than silently dropped.
pub(super) fn in_window(
    caller: &Caller,
    cx: &mut App,
    act: impl FnOnce(&mut HerdrWindow, &mut Window, &mut Context<HerdrWindow>) -> Response,
) -> Response {
    let handle = match window_for(caller, cx) {
        Ok(handle) => handle,
        Err(response) => return response,
    };
    handle
        .update(cx, |view, window, cx| act(view, window, cx))
        .unwrap_or_else(|_| Response::error(ErrorCode::NoWindow, "The window closed"))
}

/// Like [`in_window`], for actions a dialog would swallow.
pub(super) fn in_free_window(
    caller: &Caller,
    cx: &mut App,
    act: impl FnOnce(&mut HerdrWindow, &mut Window, &mut Context<HerdrWindow>) -> Response,
) -> Response {
    in_window(caller, cx, |view, window, cx| {
        if view.menu.page.is_some() {
            return Response::error(
                ErrorCode::WindowBusy,
                "A dialog is open in the window; close it first",
            );
        }
        act(view, window, cx)
    })
}

/// The window a request names with `--window`. Every request honors it or
/// is refused: one naming a window that is gone never acts elsewhere, even
/// where the work is app-wide.
fn named_window(request: &Request) -> Option<u64> {
    match request {
        Request::Open(open) => open.caller.window,
        Request::Reload(caller) | Request::Status(caller) => caller.window,
        Request::Notes(notes) => notes.caller.window,
        Request::Feedback(_) => None,
        Request::App(call) => call.caller.window,
        Request::Window(call) => call.caller.window,
        Request::Ui(call) => call.caller.window,
        Request::Command(call) => call.caller.window,
        Request::Tab(call) => call.caller.window,
        Request::Work(call) => call.caller.window,
        Request::Host(call) => call.caller.window,
        Request::Settings(call) => call.caller.window,
        Request::Events(call) => call.caller.window,
        Request::Link(call) => call.caller.window,
        Request::Chat(call) => call.caller.window,
        Request::Cloud(call) => call.caller.window,
        Request::Remote(call) => call.caller.window,
    }
}

fn serve(incoming: socket::Incoming, state: &mut State, cx: &mut App) {
    let may_park = matches!(
        incoming.request,
        Request::Tab(_)
            | Request::Work(_)
            | Request::Host(_)
            | Request::Settings(_)
            | Request::Events(_)
            | Request::Cloud(_)
            | Request::Remote(_)
    );
    if let Some(id) = named_window(&incoming.request)
        && !main_windows(cx)
            .iter()
            .any(|handle| handle.window_id().as_u64() == id)
    {
        incoming.respond(not_found(format!("Herdr UI has no window {id}")));
        return;
    }
    if may_park && state.watches.full() {
        incoming.respond(Response::error(
            ErrorCode::Busy,
            "Too many requests are waiting",
        ));
        return;
    }
    match incoming.request.clone() {
        Request::Open(request) => incoming.respond(open_browser(&request, cx)),
        Request::Reload(caller) => incoming.respond(reload(&caller, cx)),
        Request::Feedback(request) => feedback(&request, incoming, &mut state.waiters, cx),
        Request::Notes(request) => incoming.respond(notes(&request, cx)),
        Request::Status(caller) => incoming.respond(app::status(&caller, cx)),
        Request::App(call) => incoming.respond(app::app(call, cx)),
        Request::Window(call) => incoming.respond(app::window(call, cx)),
        Request::Ui(call) => incoming.respond(app::ui(call, cx)),
        Request::Command(call) => incoming.respond(app::command(call, cx)),
        Request::Tab(call) => tabs::serve(call, incoming, &mut state.watches, cx),
        Request::Work(call) => work::serve(call, incoming, &mut state.watches, cx),
        Request::Host(call) => hosts::serve(call, incoming, &mut state.watches, cx),
        Request::Settings(call) => settings::serve(call, incoming, &mut state.watches, cx),
        Request::Events(call) => events::serve(call, incoming, &mut state.watches, cx),
        Request::Link(call) => incoming.respond(links::serve(call, cx)),
        Request::Chat(call) => incoming.respond(chat::serve(call, cx)),
        Request::Cloud(call) => cloud::serve(call, incoming, &mut state.watches, cx),
        Request::Remote(call) => remote::serve(call, incoming, &mut state.watches, cx),
    }
}

fn keep_undelivered(request: super::protocol::FeedbackRequest, text: String, cx: &mut App) {
    let Some(daemon) = request.daemon_socket else {
        return;
    };
    cx.default_global::<Feedback>()
        .keep_again(crate::browser::Batch {
            target: FeedbackKey {
                scope: Scope::local(Path::new(&daemon)),
                pane_id: request.pane_id,
            },
            text,
        });
}

/// Starts answering requests. A second app instance leaves the first one's
/// socket alone and simply does not listen.
pub(crate) fn install(cx: &mut App) {
    let server = match socket_path().and_then(|path| Ok(socket::Server::bind(&path)?)) {
        Ok(server) => server,
        Err(error) => {
            tracing::warn!(%error, "Control socket unavailable");
            return;
        }
    };
    tracing::info!(path = %server.path().display(), "Control socket listening");
    let path = server.path().to_owned();
    cx.on_app_quit(move |cx| {
        let path = path.clone();
        cx.background_executor()
            .spawn(async move { socket::release(&path) })
    })
    .detach();
    let timer = cx.background_executor().clone();
    cx.spawn(async move |cx| {
        let mut state = State::default();
        loop {
            timer.timer(Duration::from_millis(50)).await;
            cx.update(|cx| {
                events::record(cx);
                serve_waiters(&mut state.waiters, cx);
                for event in server.drain() {
                    match event {
                        socket::Event::Request(incoming) if incoming.is_live() => {
                            serve(incoming, &mut state, cx);
                        }
                        socket::Event::Request(_) => {}
                        socket::Event::Undelivered { request, text } => {
                            keep_undelivered(request, text, cx);
                        }
                    }
                }
                serve_waiters(&mut state.waiters, cx);
                state.watches.serve(cx);
            });
        }
    })
    .detach();
}

#[cfg(test)]
mod tests;
