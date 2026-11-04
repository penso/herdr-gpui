//! Every child process the client and app start, built in one place.
//!
//! On Windows the app usually runs with a hidden console (its launcher starts
//! it that way), but started any other way, such as from Explorer, it may have
//! none, and a console child (git, ssh, `wsl.exe`, a shell) started from a
//! process without one gets a console window of its own. Creating every child
//! without a window means a background `git status` never flashes one,
//! however the app started. Building every command here keeps that flag from
//! being forgotten; clippy's `disallowed_methods` rejects `Command::new`
//! anywhere else.

use std::{ffi::OsStr, process::Command};

/// `CreateProcess` flag: the child gets a console with no window. Setting
/// other flags with `creation_flags` replaces this one, so include it.
#[cfg(windows)]
pub const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// A command for `program` that never opens a window of its own on Windows.
/// Elsewhere it is `Command::new`.
#[allow(clippy::disallowed_methods)]
pub fn new_command(program: impl AsRef<OsStr>) -> Command {
    #[cfg(windows)]
    {
        let mut command = Command::new(program);
        std::os::windows::process::CommandExt::creation_flags(&mut command, CREATE_NO_WINDOW);
        command
    }
    #[cfg(not(windows))]
    Command::new(program)
}

#[cfg(test)]
mod tests;
