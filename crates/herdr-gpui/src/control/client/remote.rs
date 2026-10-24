//! `remote`: the remote control daemon's status and devices, and the
//! window's pairing and revoke dialogs.
use super::{Context, Output, unexpected};
use crate::{
    cli::control::RemoteCommand,
    control::protocol::{
        Request, Response,
        remote::{RemoteOp, Reply},
    },
};
use herdr_remote_control::control::{Device, Status};

pub(super) fn remote(command: RemoteCommand, context: &Context) -> crate::Result<Output> {
    let op = match command {
        RemoteCommand::Status => RemoteOp::Status,
        RemoteCommand::Pair => RemoteOp::Pair,
        RemoteCommand::Devices => RemoteOp::Devices,
        RemoteCommand::Revoke { device } => RemoteOp::Revoke { device },
    };
    match context.call(Request::Remote, op)? {
        Response::Done { message } => Ok(Output::message(message)),
        Response::Remote { reply } => output(&reply),
        response => Err(unexpected(response)),
    }
}

/// What a reply prints, as text and as JSON.
pub(super) fn output(reply: &Reply) -> crate::Result<Output> {
    match reply {
        Reply::Status {
            enabled,
            status,
            error,
        } => {
            let json = serde_json::json!({
                "enabled": enabled,
                "running": status.is_some(),
                "status": status,
                "error": error,
            });
            Output::data(
                status_text(*enabled, status.as_deref(), error.as_deref()),
                &json,
            )
        }
        Reply::Devices { devices } => {
            let json = serde_json::json!({ "devices": devices });
            if devices.is_empty() {
                return Ok(Output::data("No paired devices", &json)?.nothing());
            }
            let now = crate::remote::now_ms();
            let lines: Vec<String> = devices.iter().map(|device| line(device, now)).collect();
            Output::data(lines.join("\n"), &json)
        }
    }
}

fn status_text(enabled: bool, status: Option<&Status>, error: Option<&str>) -> String {
    let Some(status) = status else {
        return match (enabled, error) {
            (false, _) => "Remote control is off".to_owned(),
            (true, Some(error)) => format!("Remote control is on but not running: {error}"),
            (true, None) => "Remote control is on but not running".to_owned(),
        };
    };
    let mut lines = vec![
        format!(
            "Running on port {} (version {})",
            status.port, status.version
        ),
        format!(
            "Devices: {} paired, {} connected",
            status.devices, status.connected
        ),
        format!("Chats: {}", status.chats),
    ];
    if !status.herdr {
        lines.push("Herdr is not running on this computer".to_owned());
    }
    if !status.fingerprint.is_empty() {
        lines.push(format!("Certificate authority: {}", status.fingerprint));
    }
    lines.extend(
        status
            .addresses
            .iter()
            .map(|address| format!("Address: {address}")),
    );
    lines.join("\n")
}

/// One device as a line of text: its ID for `remote revoke`, its name, and
/// how it stands.
pub(super) fn line(device: &Device, now_ms: u64) -> String {
    let seen = match device.last_seen_ms {
        _ if device.connected => "connected".to_owned(),
        Some(seen) => format!("last seen {}", crate::remote::ago(seen, now_ms)),
        None => "never connected".to_owned(),
    };
    let push = if device.push {
        "notifications on"
    } else {
        "notifications off"
    };
    format!("{}  {}  {seen}  {push}", device.id.0, device.name)
}
