//! The first process on the app's setup terminal: this executable, started
//! by the app, so it and everything it runs stay the app's children. It
//! makes the terminal its controlling one, which `ssh` needs to ask for a
//! password or a host key, then becomes the setup command. Doing this here
//! rather than between fork and exec in the app keeps it free of `unsafe`.
use std::{ffi::OsString, io, os::unix::process::CommandExt, process::ExitCode};

/// Exit status when the setup command could not start, as a shell uses.
const NOT_STARTED: u8 = 127;

pub(crate) fn run(program: &OsString, args: &[OsString]) -> ExitCode {
    if let Err(error) = lead() {
        eprintln!("Herdr could not prepare the setup terminal: {error}"); // i18n: data
        return ExitCode::from(NOT_STARTED);
    }
    // Only returns when the command could not be started.
    let error = herdr_client::new_command(program).args(args).exec();
    eprintln!(
        "Herdr could not start {}: {error}", // i18n: data
        program.to_string_lossy()
    );
    ExitCode::from(NOT_STARTED)
}

/// A new session, whose controlling terminal is standard input. Ctrl-C on
/// the terminal then reaches the setup, and hanging up the session's group
/// stops all of it.
fn lead() -> io::Result<()> {
    rustix::process::setsid()?;
    rustix::process::ioctl_tiocsctty(io::stdin())?;
    Ok(())
}
