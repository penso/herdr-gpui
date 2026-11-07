//! Registers the app with Windows Restart Manager, so an installer or Windows
//! Update that closes it to replace files, or a restart with **Restart apps**
//! (Settings > Accounts > Sign-in options) turned on, opens it again.
//!
//! Windows restarts the exact program that registered, and `herdr-gpui.exe`
//! is a console program: started on its own it opens a console window. So it
//! registers [`RESTARTED`] as its only argument, and a process started that
//! way hands off to `herdr-gpui-launcher.exe` beside it, which opens the app
//! without one, then exits. Its console may still flash on that rare path. The
//! app reopens as its Start menu entry starts it: a session or socket chosen
//! on the command line is not carried over. It is never restarted after a
//! crash or a hang, which could otherwise loop.

use std::{ffi::OsString, process::ExitCode};

/// The argument Restart Manager restarts the app with.
pub(crate) const RESTARTED: &str = "--restart-manager";

/// Call once from a normal GUI launch; fixtures and commands never register.
#[cfg(windows)]
pub(crate) fn register() {
    use windows_sys::Win32::System::Recovery::{
        RESTART_NO_CRASH, RESTART_NO_HANG, RegisterApplicationRestart,
    };
    // SAFETY: the command line is a static null-terminated UTF-16 literal, and
    // the flags are constants; Windows copies both and keeps no pointer.
    #[allow(unsafe_code)]
    let result = unsafe {
        RegisterApplicationRestart(
            windows_sys::w!("--restart-manager"),
            RESTART_NO_CRASH | RESTART_NO_HANG,
        )
    };
    if result != 0 {
        tracing::warn!(hresult = result, "Restart Manager registration failed");
    }
}

#[cfg(not(windows))]
pub(crate) fn register() {}

/// For a process Restart Manager started: drops [`RESTARTED`] from `args`
/// and, when the launcher sits beside this executable, starts it and returns
/// the exit code to end with. `None` means this process opens the app itself.
pub(crate) fn hand_off(args: &mut Vec<OsString>) -> Option<ExitCode> {
    if args.len() != 1 || args[0] != RESTARTED {
        return None;
    }
    args.clear();
    let executable = std::env::current_exe().ok()?;
    let directory = executable.parent()?;
    let launcher = ["sheprd-launcher.exe", "herdr-gpui-launcher.exe"]
        .into_iter()
        .map(|name| directory.join(name))
        .find(|path| path.is_file())?;
    if !cfg!(windows) || !launcher.is_file() {
        return None;
    }
    let started = sheprd_client::new_command(&launcher)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();
    Some(match started {
        Ok(_) => ExitCode::SUCCESS,
        Err(_) => ExitCode::FAILURE,
    })
}

#[cfg(test)]
mod tests;
