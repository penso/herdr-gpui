//! `update` and `integrations`: what the app's updater and the selected
//! host report. Nothing here downloads, installs, or runs an installer.
use super::{
    super::{main_windows, window_for},
    Watches, socket,
};
use crate::{
    HerdrWindow,
    control::protocol::{
        Caller, ErrorCode, Response,
        settings::{IntegrationInfo, Reply},
    },
    updater::{State, Updater},
};
use gpui::{App, WindowHandle};
use std::time::Duration;

/// The state's short name, its detail, and the version it names. The
/// detail is English: the command line's words are not localized.
pub(super) fn describe(state: &State) -> (&'static str, Option<String>, Option<&str>) {
    match state {
        State::Disabled(reason) => ("disabled", Some(reason.clone()), None),
        State::Idle => ("idle", None, None),
        State::Checking => ("checking", None, None),
        State::Current => ("current", None, None),
        State::Available { version, .. } => ("available", None, Some(version)),
        State::Downloading { .. } => ("downloading", None, None),
        State::Ready { version } => ("ready", None, Some(version)),
        State::Installing => ("installing", None, None),
        State::Homebrew { version, .. } => ("homebrew", None, Some(version)),
        State::Upgrading { detail } => ("upgrading", Some(detail.to_string()), None),
        State::Restart { version } => ("restart", None, Some(version)),
        State::Restarting => ("restarting", None, None),
        State::Cancelling => ("cancelling", None, None),
        State::Error(message) => ("error", Some(message.clone()), None),
    }
}

fn reply(updater: &Updater) -> Response {
    let (state, detail, version) = describe(updater.state());
    Response::Settings {
        reply: Reply::Update {
            state: state.to_owned(),
            latest: version.or(updater.latest_version()).map(str::to_owned),
            detail,
        },
    }
}

/// The window that runs the app's updater: the only one whose updater is
/// not disabled, or else the first opened, whose reason is the real one.
fn updater_window(cx: &mut App) -> Option<WindowHandle<HerdrWindow>> {
    let mut windows = main_windows(cx);
    windows.sort_by_key(|handle| handle.window_id().as_u64());
    let running = windows.iter().copied().find(|handle| {
        handle
            .read_with(cx, |view, _| {
                !matches!(view.updater.state(), State::Disabled(_))
            })
            .unwrap_or(false)
    });
    running.or_else(|| windows.first().copied())
}

fn no_window() -> Response {
    Response::error(ErrorCode::NoWindow, "Herdr UI has no window open")
}

pub(super) fn update(cx: &mut App) -> Response {
    let Some(handle) = updater_window(cx) else {
        return no_window();
    };
    handle
        .read_with(cx, |view, _| reply(&view.updater))
        .unwrap_or_else(|_| no_window())
}

pub(super) fn check_update(
    wait: Duration,
    incoming: socket::Incoming,
    watches: &mut Watches,
    cx: &mut App,
) {
    let Some(handle) = updater_window(cx) else {
        incoming.respond(no_window());
        return;
    };
    // Some refusal, or none once the check is asked for.
    let refused = handle.update(cx, |view, _, _| {
        if let State::Disabled(reason) = view.updater.state() {
            return Some(Response::error(
                ErrorCode::Unsupported,
                format!("Updates are off in this build: {reason}"),
            ));
        }
        // A busy updater, such as one downloading, keeps what it is doing.
        view.updater.check();
        None
    });
    match refused {
        Ok(None) => {}
        Ok(Some(response)) => {
            incoming.respond(response);
            return;
        }
        Err(_) => {
            incoming.respond(no_window());
            return;
        }
    }
    watches.add(
        incoming,
        wait,
        move |cx| match handle.read_with(cx, |view, _| {
            (!matches!(view.updater.state(), State::Checking)).then(|| reply(&view.updater))
        }) {
            Ok(answer) => answer,
            Err(_) => Some(no_window()),
        },
        Response::error(ErrorCode::Timeout, "No answer about the latest release yet"),
        cx,
    );
}

pub(super) fn integrations(
    caller: &Caller,
    wait: Duration,
    incoming: socket::Incoming,
    watches: &mut Watches,
    cx: &mut App,
) {
    let handle = match window_for(caller, cx) {
        Ok(handle) => handle,
        Err(response) => {
            incoming.respond(response);
            return;
        }
    };
    if handle
        .update(cx, |view, _, cx| view.load_integrations(cx))
        .is_err()
    {
        incoming.respond(no_window());
        return;
    }
    watches.add(
        incoming,
        wait,
        move |cx| match handle.read_with(cx, |view, _| listed(view)) {
            Ok(answer) => answer,
            Err(_) => Some(Response::error(ErrorCode::NoWindow, "The window closed")),
        },
        Response::error(
            ErrorCode::Timeout,
            "The host did not list its integrations in time",
        ),
        cx,
    );
}

fn listed(view: &HerdrWindow) -> Option<Response> {
    let integrations = &view.integrations;
    if integrations.busy {
        return None;
    }
    if let Some(error) = &integrations.error {
        return Some(Response::error(ErrorCode::Failed, error.clone()));
    }
    Some(Response::Settings {
        reply: Reply::Integrations {
            integrations: integrations
                .listed()
                .map(|(id, label, status)| IntegrationInfo {
                    id: id.to_owned(),
                    label: label.to_owned(),
                    status: status.to_owned(),
                })
                .collect(),
        },
    })
}
