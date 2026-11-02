//! A server on a host, reached through the SSH that runs it: the remote
//! script listens on a Unix socket, and the same `ssh` child forwards a
//! loopback port here to it. Ending the child closes both the forward and
//! the script's standard input, which is how such a script learns to stop.
use super::{HostKeys, noninteractive, quote};
use crate::{Result, catalog::validate_target};
use std::process::Command;

/// Where a socket named `name` lies on the host.
pub fn remote_socket(name: &str) -> String {
    format!("/tmp/{name}.sock")
}

/// A noninteractive `ssh` child that runs `script` under the remote
/// `/bin/sh`, with `$socket` set to [`remote_socket`]`(name)`, and forwards
/// `127.0.0.1:<local>` on this machine to that socket. `name` is the app's
/// own: lowercase letters, digits, and hyphens, as a random name is written.
///
/// `ExitOnForwardFailure` ends the child when it cannot bind the local port,
/// and SSH binds it before it runs the script, so anything the script prints
/// means this child holds the port. `ControlPath=none` keeps the forward in
/// this child, so killing it closes the forward. The caller owns the child:
/// its streams, readiness, and reaping.
pub fn serve_command(target: &str, local: u16, name: &str, script: &str) -> Result<Command> {
    validate_target(target)?;
    let valid = |byte: u8| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-';
    if name.is_empty() || name.len() > 64 || !name.bytes().all(valid) {
        return Err(super::Error::InvalidSocket.into());
    }
    let socket = remote_socket(name);
    let script = format!("socket={}\n{script}", quote(&socket));
    let mut command = noninteractive(HostKeys::of(target));
    command.args([
        "-o",
        "ControlPath=none",
        "-o",
        "ExitOnForwardFailure=yes",
        "-L",
        &format!("127.0.0.1:{local}:{socket}"),
        "--",
        target,
        &format!("/bin/sh -c {}", quote(&script)),
    ]);
    Ok(command)
}
