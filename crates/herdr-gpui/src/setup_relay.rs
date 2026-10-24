//! Device setups that need prompts run as this app's children, on a terminal
//! the app owns, and a daemon workspace only shows them.
//!
//! macOS Local Network privacy judges a process by its responsible code
//! (Apple's TN3179). A process this app starts is the app's: it shares the
//! permission the user gave Herdr. A process in a Herdr daemon's pane is not:
//! a daemon started outside this app leaves each one responsible for itself,
//! so a third-party tool such as Homebrew's `ssh` there is refused a LAN host
//! with "no route to host", without an alert and without an entry the user
//! could turn on, while Apple's own tools are let through. Running the setup
//! here keeps the user's own `ssh`, configuration, agent, and security key,
//! and the prompts, confirmations, and host trust of `ssh-copy-id` and
//! `herdr machine add`, unchanged.
//!
//! The pane runs this executable with [`ATTACH`], which relays its terminal
//! over a private socket to [`host`]; the setup itself starts through
//! [`SESSION`]. The setup ends with this app: quitting it hangs the setup up
//! as closing a terminal would.

mod attach;
mod error;
mod frame;
mod host;
mod session;

pub(crate) use error::Error;
pub(crate) use host::{Listening, listen};

use std::{ffi::OsString, path::Path, process::ExitCode};

/// The argument that makes this executable the pane's end of a setup.
pub(crate) const ATTACH: &str = "__device-setup-attach";
/// The argument that makes this executable the first process of a setup.
pub(crate) const SESSION: &str = "__device-setup-session";

/// Runs this executable as one end of a setup when `args` asks for it, as
/// the updater's helper does, before anything of the app starts.
pub(crate) fn run_helper(args: &[OsString]) -> Option<ExitCode> {
    let (first, rest) = args.split_first()?;
    if first == ATTACH {
        return Some(match rest {
            [path] if !path.is_empty() => attach::run(Path::new(path)),
            _ => usage(ATTACH, "SOCKET"),
        });
    }
    if first == SESSION {
        return Some(match rest.split_first() {
            Some((program, args)) => session::run(program, args),
            None => usage(SESSION, "PROGRAM [ARGS...]"),
        });
    }
    None
}

fn usage(helper: &str, arguments: &str) -> ExitCode {
    eprintln!("Usage: herdr-gpui {helper} {arguments}");
    ExitCode::from(2)
}

#[cfg(test)]
mod tests;
