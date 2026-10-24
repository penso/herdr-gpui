//! `status`, `app`, `window`, `ui`, and `run`.
use super::{in_free_window, in_window, main_windows, window_for};
use crate::{
    HerdrWindow,
    constants::APP_VERSION,
    control::protocol::{
        Call, Caller, ErrorCode, PROTOCOL_VERSION, Response,
        app::{AppOp, CommandOp, FontSize, HostInfo, Reply, Status, UiOp, WindowInfo, WindowOp},
    },
    controls::{COMMANDS, Command},
};
use gpui::{App, Window, px, size};
use herdr_client::ConnectTarget;
use std::time::Duration;

/// How long `app quit` waits, so its answer reaches the caller first.
const QUIT_DELAY: Duration = Duration::from_millis(200);

impl HerdrWindow {
    /// The hosts this window shows, with their connection status.
    pub(crate) fn control_hosts(&self) -> Vec<HostInfo> {
        self.endpoints
            .iter()
            .enumerate()
            .map(|(index, endpoint)| {
                let selected = index == self.selected_endpoint;
                // The selected host's live state is the window's own.
                let status = match endpoint.status() {
                    "online" | "connecting" if selected => {
                        if self.live.status.is_connected() {
                            "online"
                        } else {
                            "connecting"
                        }
                    }
                    status => status,
                };
                HostInfo {
                    id: endpoint.id.clone(),
                    label: endpoint.label.clone(),
                    status: status.to_owned(),
                    selected,
                }
            })
            .collect()
    }

    fn control_info(&self, window: &Window) -> WindowInfo {
        let bounds = window.bounds();
        WindowInfo {
            id: window.window_handle().window_id().as_u64(),
            title: self.title.clone(),
            active: window.is_window_active(),
            fullscreen: window.is_fullscreen(),
            width: f32::from(bounds.size.width),
            height: f32::from(bounds.size.height),
            workspace: self
                .live
                .snapshot
                .as_ref()
                .and_then(|snapshot| snapshot.focused_workspace_id.clone()),
            hosts: self.control_hosts(),
        }
    }
}

fn windows(cx: &mut App) -> Vec<WindowInfo> {
    main_windows(cx)
        .into_iter()
        .filter_map(|handle| {
            handle
                .update(cx, |view, window, _| view.control_info(window))
                .ok()
        })
        .collect()
}

pub(super) fn status(_: &Caller, cx: &mut App) -> Response {
    Response::Status(Status {
        version: APP_VERSION.to_owned(),
        protocol: PROTOCOL_VERSION,
        pid: std::process::id(),
        browser_tabs: crate::browser::EMBEDDED,
        windows: windows(cx),
    })
}

/// Brings `handle`'s window forward. Deferred: activating a window updates
/// it, which cannot happen inside another update.
fn activate(handle: gpui::WindowHandle<HerdrWindow>, cx: &mut App) {
    cx.activate(true);
    cx.defer(move |cx| {
        let _ = handle.update(cx, |_, window, _| window.activate_window());
    });
}

pub(super) fn app(call: Call<AppOp>, cx: &mut App) -> Response {
    match call.op {
        AppOp::Focus => match window_for(&call.caller, cx) {
            Ok(handle) => {
                activate(handle, cx);
                Response::done("Brought Herdr UI forward")
            }
            Err(response) => response,
        },
        AppOp::Open { path } => open(&call.caller, path, cx),
        AppOp::Hide => {
            cx.hide();
            Response::done("Hid Herdr UI")
        }
        AppOp::Quit => {
            let timer = cx.background_executor().clone();
            cx.spawn(async move |cx| {
                timer.timer(QUIT_DELAY).await;
                cx.update(|cx| cx.quit());
            })
            .detach();
            Response::done("Quitting Herdr UI; Herdr and its terminals keep running")
        }
    }
}

/// `open DIR`: the folder's workspace, focused or created through the
/// palette's Open Project, in the window brought forward. The caller
/// resolved the path; the palette checks it again before Herdr sees it.
fn open(caller: &Caller, path: String, cx: &mut App) -> Response {
    let handle = match window_for(caller, cx) {
        Ok(handle) => handle,
        Err(response) => return response,
    };
    let opened = in_free_window(caller, cx, |view, window, cx| {
        open_in(view, path, window, cx)
    });
    if matches!(opened, Response::Done { .. }) {
        activate(handle, cx);
    }
    opened
}

/// The window's part of `open`, once no dialog holds it.
fn open_in(
    view: &mut HerdrWindow,
    path: String,
    window: &mut Window,
    cx: &mut gpui::Context<HerdrWindow>,
) -> Response {
    match view.open_project_at(path.clone().into(), window, cx) {
        Ok(()) => Response::done(format!("Opening {path} in Herdr UI")),
        // The window has not reached its local Herdr yet, as right after the
        // app starts; a caller may try again shortly.
        Err(crate::Error::Palette(
            crate::palette::Error::ConnectionNotReady | crate::palette::Error::HostUnavailable,
        )) => Response::error(
            ErrorCode::Busy,
            "The window is not connected to the local Herdr yet",
        ),
        Err(error) => Response::error(ErrorCode::Failed, error.to_string()),
    }
}

/// What `window new` connects to: the launch target of the window
/// `--window` names; else the caller's own daemon when it runs in a Herdr
/// pane, as a window that knows it names it or else by its socket; outside
/// Herdr, the frontmost window's launch target.
fn new_window_target(caller: &Caller, cx: &mut App) -> Result<ConnectTarget, Response> {
    if caller.window.is_some() {
        let handle = window_for(caller, cx)?;
        return handle
            .read_with(cx, |view, _| view.endpoints[0].connection.target.clone())
            .map_err(|_| Response::error(ErrorCode::NoWindow, "The window closed"));
    }
    if let Some(daemon) = caller.daemon_socket.as_deref() {
        let daemon = std::path::Path::new(daemon);
        let known = main_windows(cx).into_iter().find_map(|handle| {
            handle
                .read_with(cx, |view, _| {
                    view.endpoints
                        .iter()
                        .find(|endpoint| {
                            endpoint.connection.target.socket_path().ok().as_deref() == Some(daemon)
                        })
                        .map(|endpoint| endpoint.connection.target.clone())
                })
                .ok()
                .flatten()
        });
        return Ok(known.unwrap_or_else(|| ConnectTarget::Socket(daemon.to_owned())));
    }
    Ok(main_windows(cx)
        .first()
        .and_then(|handle| {
            handle
                .read_with(cx, |view, _| view.endpoints[0].connection.target.clone())
                .ok()
        })
        .unwrap_or(ConnectTarget::Local))
}

pub(super) fn window(call: Call<WindowOp>, cx: &mut App) -> Response {
    let caller = &call.caller;
    match call.op {
        WindowOp::List => Response::App {
            reply: Reply::Windows {
                windows: windows(cx),
            },
        },
        WindowOp::New { session } => {
            let target = match session {
                Some(name) => {
                    if !super::plain(&name, 128) {
                        return Response::error(ErrorCode::InvalidRequest, "Invalid session name");
                    }
                    ConnectTarget::Session {
                        name,
                        development: false,
                    }
                }
                None => match new_window_target(caller, cx) {
                    Ok(target) => target,
                    Err(response) => return response,
                },
            };
            crate::open_additional_window(target, cx);
            // Opened after this answer; a failure is only logged.
            Response::done("Asked for a new window; `window list` shows it once open")
        }
        WindowOp::Focus => match window_for(caller, cx) {
            Ok(handle) => {
                activate(handle, cx);
                Response::done(format!("Focused window {}", handle.window_id().as_u64()))
            }
            Err(response) => response,
        },
        WindowOp::Fullscreen { switch } => in_window(caller, cx, |_, window, _| {
            let now = window.is_fullscreen();
            if switch.apply(now) != now {
                window.toggle_fullscreen();
            }
            Response::done(if switch.apply(now) {
                "Full screen"
            } else {
                "Not full screen"
            })
        }),
        WindowOp::Size { size: None } => in_window(caller, cx, |_, window, _| {
            let bounds = window.bounds();
            Response::App {
                reply: Reply::Size {
                    width: f32::from(bounds.size.width),
                    height: f32::from(bounds.size.height),
                },
            }
        }),
        WindowOp::Size { size: Some(asked) } => in_window(caller, cx, |_, window, _| {
            window.resize(size(px(asked.width as f32), px(asked.height as f32)));
            Response::done(format!("Resized to {}x{}", asked.width, asked.height))
        }),
        WindowOp::Detach => in_free_window(caller, cx, |view, window, cx| {
            view.command(Command::Detach, window, cx);
            Response::done("Detached from the selected host; it keeps running")
        }),
        WindowOp::Reconnect => in_free_window(caller, cx, |view, window, cx| {
            view.command(Command::Reconnect, window, cx);
            Response::done("Reconnecting to the selected host")
        }),
    }
}

pub(super) fn ui(call: Call<UiOp>, cx: &mut App) -> Response {
    in_window(&call.caller, cx, |view, window, cx| match call.op {
        UiOp::Sidebar { switch } => {
            let visible = switch.apply(view.sidebar_visible);
            if visible != view.sidebar_visible {
                view.toggle_sidebar();
                cx.notify();
            }
            Response::done(if visible {
                "Sidebar shown"
            } else {
                "Sidebar hidden"
            })
        }
        UiOp::Panel { panel } => {
            if !view.activity_bar_shown() {
                return Response::error(
                    ErrorCode::Unsupported,
                    "The activity bar is off: [sidebar] activity_bar = false",
                );
            }
            if !view.activity_panel_listed(panel) {
                return Response::error(
                    ErrorCode::Unsupported,
                    format!(
                        "{} is turned off: [sidebar] {} = false",
                        panel.name(),
                        panel.name()
                    ),
                );
            }
            view.show_activity_panel(panel, window, cx);
            Response::done(format!("Showing {}", panel.name()))
        }
        UiOp::StatusBar { switch } => {
            let visible = switch.apply(view.status_bar_visible);
            if visible != view.status_bar_visible {
                view.status_bar_visible = visible;
                cx.notify();
            }
            Response::done(if visible {
                "Status bar shown"
            } else {
                "Status bar hidden"
            })
        }
        UiOp::FontSize { size } => {
            let step = crate::config::FONT_SIZE_STEP;
            let points = match size {
                FontSize::Larger => view.config.terminal.size + step,
                FontSize::Smaller => view.config.terminal.size - step,
                FontSize::Reset => view.configured_terminal_size,
                FontSize::Points { points } => f32::from(points),
            };
            view.set_terminal_font_size(points, cx);
            Response::App {
                reply: Reply::FontSize {
                    points: view.config.terminal.size,
                },
            }
        }
    })
}

pub(super) fn command(call: Call<CommandOp>, cx: &mut App) -> Response {
    let CommandOp::Run { name } = call.op;
    let Some(info) = COMMANDS.iter().find(|info| info.name == name) else {
        return super::not_found(format!("No command is named {name}"));
    };
    match info.command {
        // Quitting answers first, as `app quit` does.
        Command::Quit => app(
            Call {
                caller: call.caller,
                op: AppOp::Quit,
            },
            cx,
        ),
        command => in_free_window(&call.caller, cx, |view, window, cx| {
            view.command(command, window, cx);
            Response::done(format!("Ran {}", info.label))
        }),
    }
}

#[cfg(test)]
mod tests;
