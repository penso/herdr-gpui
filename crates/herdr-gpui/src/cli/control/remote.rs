//! `remote`: remote control from a phone.
use super::{Args, CliError, unknown};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RemoteCommand {
    /// Whether the daemon runs, where, and who is connected.
    Status,
    /// Open the pairing dialog; never prints a token.
    Pair,
    Devices,
    /// Open the confirmation for revoking a device, by ID or name.
    Revoke {
        device: String,
    },
}

pub(super) fn remote(args: &mut Args) -> Result<RemoteCommand, CliError> {
    let Some(verb) = args.verb() else {
        return Ok(RemoteCommand::Status);
    };
    Ok(match verb.to_str() {
        Some("status") => RemoteCommand::Status,
        Some("pair") => RemoteCommand::Pair,
        Some("devices" | "list") => RemoteCommand::Devices,
        // A name may be typed unquoted, as `remote revoke Fabien's iPhone`.
        Some("revoke") => RemoteCommand::Revoke {
            device: args.words()?.ok_or(CliError::MissingArgument {
                command: "remote revoke",
                what: "DEVICE",
            })?,
        },
        _ => return Err(unknown("remote", verb)),
    })
}
