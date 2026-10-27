//! `remote status`, `remote devices`, `remote pair`, `remote revoke`, and
//! `remote restart`.
//! The daemon is asked on the background executor. Pairing and revoking
//! open the window's own dialogs, where the user approves a device or
//! confirms the revoke; neither happens from here.
use super::{Watches, in_free_window, settings::answer_when_done, socket, window_for};
use crate::{
    control::protocol::{
        Call, ErrorCode, Response,
        remote::{RemoteOp, Reply},
    },
    remote::Supervisor,
};
use gpui::App;
use herdr_remote_control::control::{Device, Request};
use std::time::Duration;

const OFF: &str = "Remote control is off; turn it on in Settings > Remote control";

pub(super) fn serve(
    call: Call<RemoteOp>,
    incoming: socket::Incoming,
    watches: &mut Watches,
    cx: &mut App,
) {
    let wait = Duration::from_secs(call.op.wait_seconds());
    let Call { caller, op } = call;
    let enabled = match window_for(&caller, cx) {
        Ok(handle) => handle
            .read_with(cx, |view, _| view.config.remote.enabled)
            .unwrap_or(false),
        Err(response) => return incoming.respond(response),
    };
    if let RemoteOp::Pair { address } = op {
        return incoming.respond(in_free_window(&caller, cx, |view, window, cx| {
            if !enabled {
                return Response::error(ErrorCode::Failed, OFF);
            }
            if view.open_remote_pairing_at(address, window, cx) {
                Response::done(
                    "Showing the pairing dialog in Herdr UI; the device is approved there",
                )
            } else {
                Response::error(
                    ErrorCode::WindowBusy,
                    "A dialog is open in the window; close it first",
                )
            }
        }));
    }
    if !enabled && op != RemoteOp::Status {
        return incoming.respond(Response::error(ErrorCode::Failed, OFF));
    }
    if op == RemoteOp::Restart {
        return incoming.respond(restart(cx));
    }
    let Some(client) = Supervisor::client(cx) else {
        return incoming.respond(Response::error(
            ErrorCode::Unsupported,
            "This Herdr UI does not run remote control",
        ));
    };
    let task = match op {
        RemoteOp::Status if !enabled => {
            return incoming.respond(Response::Remote {
                reply: Reply::Status {
                    enabled,
                    status: None,
                    error: None,
                },
            });
        }
        RemoteOp::Status => {
            let request = client.request(Request::Status);
            cx.spawn(async move |_| {
                let (status, error) = match request.await {
                    Ok(herdr_remote_control::control::Response::Status(status)) => {
                        (Some(Box::new(status)), None)
                    }
                    Ok(_) => (None, Some(crate::remote::Error::Unexpected.to_string())),
                    Err(error) => (None, Some(error.to_string())),
                };
                Response::Remote {
                    reply: Reply::Status {
                        enabled,
                        status,
                        error,
                    },
                }
            })
        }
        RemoteOp::Devices => {
            let request = client.request(Request::Devices);
            cx.spawn(async move |_| match devices(request.await) {
                Ok(devices) => Response::Remote {
                    reply: Reply::Devices { devices },
                },
                Err(response) => response,
            })
        }
        RemoteOp::Revoke { device } => {
            let request = client.request(Request::Devices);
            cx.spawn(async move |cx| {
                let devices = match devices(request.await) {
                    Ok(devices) => devices,
                    Err(response) => return response,
                };
                let device = match pick(&devices, &device) {
                    Ok(device) => device.clone(),
                    Err(response) => return response,
                };
                cx.update(|cx| {
                    in_free_window(&caller, cx, |view, window, cx| {
                        let name = crate::notifications::safe_text(&device.name, 64);
                        if view.ask_to_revoke(device, window, cx) {
                            Response::done(format!(
                                "Asking in Herdr UI to revoke {name}; the user confirms there"
                            ))
                        } else {
                            Response::error(
                                ErrorCode::WindowBusy,
                                "A dialog is open in the window; close it first",
                            )
                        }
                    })
                })
            })
        }
        RemoteOp::Pair { .. } | RemoteOp::Restart => return,
    };
    answer_when_done(task, wait, incoming, watches, cx);
}

/// Starts the restart; its outcome shows in Settings like any service
/// change's.
fn restart(cx: &mut App) -> Response {
    if !cx.has_global::<Supervisor>() {
        return Response::error(
            ErrorCode::Unsupported,
            "This Herdr UI does not run remote control",
        );
    }
    if Supervisor::restart(cx) {
        Response::done("Restarting the remote control daemon; phones reconnect on their own")
    } else {
        Response::error(
            ErrorCode::WindowBusy,
            "The remote control service is changing now; try again shortly",
        )
    }
}

fn devices(
    answer: Result<herdr_remote_control::control::Response, herdr_remote_control::control::Error>,
) -> Result<Vec<Device>, Response> {
    match answer {
        Ok(herdr_remote_control::control::Response::Devices { devices }) => Ok(devices),
        Ok(_) => Err(Response::error(
            ErrorCode::Failed,
            crate::remote::Error::Unexpected.to_string(),
        )),
        Err(error) => Err(Response::error(ErrorCode::Failed, error.to_string())),
    }
}

/// The device `needle` names: by ID, else the one device of that name.
pub(super) fn pick<'a>(devices: &'a [Device], needle: &str) -> Result<&'a Device, Response> {
    if let Some(device) = devices.iter().find(|device| device.id.0 == needle) {
        return Ok(device);
    }
    let mut named = devices.iter().filter(|device| device.name == needle);
    match (named.next(), named.next()) {
        (Some(device), None) => Ok(device),
        (Some(_), Some(_)) => Err(Response::error(
            ErrorCode::InvalidRequest,
            format!("Several devices are named {needle}; name one by its ID from `remote devices`"),
        )),
        (None, _) => Err(Response::error(
            ErrorCode::NotFound,
            format!("No paired device is named {needle}"),
        )),
    }
}

#[cfg(test)]
mod tests;
