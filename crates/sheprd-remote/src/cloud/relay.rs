//! This executable started as a provider's exec relay.

use std::{ffi::OsString, io, process::Command};

/// This executable run as the relay `helper`, for the arguments a provider
/// appends. The relay reads its provider's table before it could adopt the
/// login shell's `XDG_CONFIG_HOME` itself, so it runs under the
/// configuration root this app resolved, as the daemon launch does.
pub(crate) fn command(helper: &str) -> io::Result<Command> {
    let mut command = sheprd_client::new_command(std::env::current_exe()?);
    command.arg(helper);
    pin_config_home(&mut command, sheprd_client::local_var("XDG_CONFIG_HOME"));
    Ok(command)
}

/// Run `command` under `root`, or without an inherited `XDG_CONFIG_HOME`
/// when this app uses the default root.
pub(super) fn pin_config_home(command: &mut Command, root: Option<OsString>) {
    match root {
        Some(root) => command.env("XDG_CONFIG_HOME", root),
        None => command.env_remove("XDG_CONFIG_HOME"),
    };
}
