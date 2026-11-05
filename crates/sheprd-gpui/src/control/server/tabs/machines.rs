//! `machine list`, `set`, `suspend`, and `delete`. Deleting asks in Settings:
//! it destroys what is on the machine, so the user confirms there.

use super::*;
#[cfg(target_os = "macos")]
use crate::{
    SheprdWindow,
    control::{
        protocol::tabs::{MachineInfo, MachineState},
        server::window_for,
    },
    i18n::Localize as _,
    machine::{self, Limits, Listing, Name, Settings, Status},
};

/// What `machine set` changes: CPUs, memory in GB, and the display.
type Changes = (Option<u16>, Option<u16>, Option<String>);

#[cfg(not(target_os = "macos"))]
fn unsupported(incoming: socket::Incoming) {
    incoming.respond(Response::error(
        ErrorCode::Unsupported,
        "Virtual Macs need an Apple silicon Mac",
    ));
}

#[cfg(not(target_os = "macos"))]
pub(super) fn list(incoming: socket::Incoming, _: &mut App) {
    unsupported(incoming);
}

#[cfg(not(target_os = "macos"))]
pub(super) fn set(_: String, _: Changes, incoming: socket::Incoming, _: &mut App) {
    unsupported(incoming);
}

#[cfg(not(target_os = "macos"))]
pub(super) fn suspend(_: String, incoming: socket::Incoming, _: &mut App) {
    unsupported(incoming);
}

#[cfg(not(target_os = "macos"))]
pub(super) fn delete(_: &Caller, _: String, incoming: socket::Incoming, _: &mut App) {
    unsupported(incoming);
}

#[cfg(target_os = "macos")]
fn named(name: String, incoming: socket::Incoming) -> Option<(Name, socket::Incoming)> {
    match Name::try_from(name) {
        Ok(name) => Some((name, incoming)),
        Err(error) => {
            incoming.respond(Response::error(
                ErrorCode::InvalidRequest,
                error.to_string(),
            ));
            None
        }
    }
}

#[cfg(target_os = "macos")]
fn info(listing: Listing) -> MachineInfo {
    let state = match (machine::status(&listing.name), listing.saved) {
        (Status::Running, _) => MachineState::Running,
        (Status::Starting, _) => MachineState::Starting,
        (Status::Saving, _) => MachineState::Saving,
        (Status::Failed(_), _) => MachineState::Failed,
        (Status::Stopped, true) => MachineState::Suspended,
        (Status::Stopped, false) => MachineState::Stopped,
    };
    MachineInfo {
        name: listing.name.to_string(),
        state,
        cpus: listing.settings.cpus,
        memory_gib: listing.settings.memory_gib,
        display: listing.settings.display.to_string(),
        disk_bytes: listing.disk_bytes,
    }
}

#[cfg(target_os = "macos")]
fn failed(error: &machine::Error) -> Response {
    Response::error(ErrorCode::Failed, error.to_string())
}

#[cfg(target_os = "macos")]
pub(super) fn list(incoming: socket::Incoming, cx: &mut App) {
    let read = cx.background_executor().spawn(async { machine::list() });
    cx.spawn(async move |cx: &mut gpui::AsyncApp| {
        let listed = read.await;
        let response = cx.update(|_| match listed {
            Ok(listings) => Response::Tab {
                reply: Reply::Machines {
                    machines: listings.into_iter().map(info).collect(),
                },
            },
            Err(error) => failed(&error),
        });
        incoming.respond(response);
    })
    .detach();
}

/// `settings` with `changes`, checked against this Mac.
#[cfg(target_os = "macos")]
fn changed(settings: Settings, changes: Changes) -> machine::Result<Settings> {
    let (cpus, memory_gib, display) = changes;
    let settings = Settings {
        cpus: cpus.unwrap_or(settings.cpus),
        memory_gib: memory_gib.unwrap_or(settings.memory_gib),
        display: display.map_or(Ok(settings.display), |display| display.parse())?,
    };
    Limits::host().check(&settings)?;
    Ok(settings)
}

#[cfg(target_os = "macos")]
pub(super) fn set(name: String, changes: Changes, incoming: socket::Incoming, cx: &mut App) {
    let Some((name, incoming)) = named(name, incoming) else {
        return;
    };
    let write = cx.background_executor().spawn(async move {
        let settings = changed(machine::settings(&name)?, changes)?;
        machine::save_settings(&name, &settings)?;
        Ok::<_, machine::Error>(name)
    });
    cx.spawn(async move |_: &mut gpui::AsyncApp| {
        incoming.respond(match write.await {
            Ok(name) => Response::done(format!("{name} takes these when it next boots")),
            Err(error @ machine::Error::NotInstalled(_)) => {
                Response::error(ErrorCode::NotFound, error.to_string())
            }
            Err(
                error @ (machine::Error::Cpus { .. }
                | machine::Error::Memory { .. }
                | machine::Error::InvalidDisplay),
            ) => Response::error(ErrorCode::InvalidRequest, error.localize()),
            Err(error) => failed(&error),
        });
    })
    .detach();
}

#[cfg(target_os = "macos")]
pub(super) fn suspend(name: String, incoming: socket::Incoming, cx: &mut App) {
    let Some((name, incoming)) = named(name, incoming) else {
        return;
    };
    if machine::status(&name) != Status::Running {
        return incoming.respond(Response::error(
            ErrorCode::Failed,
            format!("{name} is not running"), // i18n: data
        ));
    }
    machine::suspend(&name, cx);
    incoming.respond(Response::done(format!("Saving {name}")));
}

#[cfg(target_os = "macos")]
pub(super) fn delete(caller: &Caller, name: String, incoming: socket::Incoming, cx: &mut App) {
    let Some((name, incoming)) = named(name, incoming) else {
        return;
    };
    let handle = match window_for(caller, cx) {
        Ok(handle) => handle,
        Err(response) => return incoming.respond(response),
    };
    let read = cx.background_executor().spawn(async { machine::list() });
    cx.spawn(async move |cx: &mut gpui::AsyncApp| {
        let found = read
            .await
            .is_ok_and(|listings| listings.iter().any(|listing| listing.name == name));
        if !found {
            return incoming.respond(Response::error(
                ErrorCode::NotFound,
                format!("No virtual Mac is called {name}"), // i18n: data
            ));
        }
        let message = format!("Asked in Settings to delete {name}"); // i18n: data
        cx.update(|cx| {
            let source = handle
                .entity(cx)
                .map(|view: gpui::Entity<SheprdWindow>| view.downgrade());
            if let Ok(source) = source {
                crate::settings_window::open_machines(source, Some(name), cx);
            }
        });
        incoming.respond(Response::done(message));
    })
    .detach();
}
