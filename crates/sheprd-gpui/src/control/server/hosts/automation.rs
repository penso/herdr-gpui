//! `automation approve`: opens the app's approval dialog for one script,
//! pointed at the host the caller names or runs on. It never approves:
//! the user reads the plan and confirms in the dialog.

use super::Place;
use crate::{
    SheprdWindow,
    automations::{Error, Machine},
    control::protocol::{Caller, ErrorCode, Response},
};
use gpui::{Context, Window};

pub(super) fn approve(
    view: &mut SheprdWindow,
    caller: &Caller,
    path: &str,
    host: Option<&str>,
    window: &mut Window,
    cx: &mut Context<SheprdWindow>,
) -> Response {
    if !view.activity_panel_listed(crate::sidebar::ActivityPanel::Automations) {
        return refused(&Error::Off);
    }
    let machine = match host {
        Some(name) => match named(view, name) {
            Some(machine) => machine,
            None => return refused(&Error::UnknownHost(name.to_owned())),
        },
        None => callers(view, caller),
    };
    match view.ask_to_approve_automation(&machine, path, window, cx) {
        Ok(true) => {
            super::super::work::bring_forward(window, cx);
            let host = view
                .automations
                .host(&machine)
                .map_or_else(String::new, |host| host.label.clone());
            Response::done(format!(
                "Asked to approve {path} on {host}; it is approved only once the user confirms"
            ))
        }
        Ok(false) => Response::error(
            ErrorCode::WindowBusy,
            "A dialog is open in the window; close it first",
        ),
        Err(error) => {
            if matches!(error, Error::NotRead { .. }) {
                view.automations.refresh();
            }
            refused(&error)
        }
    }
}

/// The machine a host named by endpoint ID, or else by label, reaches.
fn named(view: &SheprdWindow, name: &str) -> Option<Machine> {
    let hosts = &view.automations.hosts;
    hosts
        .iter()
        .find(|host| host.endpoint.as_deref() == Some(name))
        .or_else(|| hosts.iter().find(|host| host.label == name))
        .map(|host| host.machine.clone())
}

/// The machine the caller's workspace runs on, or this one.
fn callers(view: &SheprdWindow, caller: &Caller) -> Machine {
    Place::of(view, caller)
        .ok()
        .and_then(|place| Machine::of(&view.endpoints[place.index].connection.target))
        .unwrap_or(Machine::Local)
}

pub(super) fn code(error: &Error) -> ErrorCode {
    match error {
        Error::UnknownHost(_) | Error::NoScript { .. } => ErrorCode::NotFound,
        Error::NotRead { .. } => ErrorCode::Busy,
        Error::Off | Error::UnsupportedHost => ErrorCode::Unsupported,
        _ => ErrorCode::Failed,
    }
}

fn refused(error: &Error) -> Response {
    Response::error(code(error), error.to_string())
}
