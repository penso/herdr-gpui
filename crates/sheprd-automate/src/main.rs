//! `herdr-automate`: runs approved automation scripts on a host. A daemon
//! (`serve`) schedules them and runs their steps through Herdr; the other
//! commands read and change its state, as text for people or JSON for
//! Sheprd.

mod approve;
mod cli;
mod daemon;
mod error;
mod fingerprint;
mod herdr;
mod logbook;
mod paths;
mod process;
mod report;
mod runner;
mod sampler;
mod scan;
mod service;
mod settings;
mod steps;
mod store;
#[cfg(all(test, unix))]
#[allow(clippy::unwrap_used)]
mod testing;
mod usage;

pub use error::{Error, Result};

use std::{io::Write, process::ExitCode};

fn main() -> ExitCode {
    let command = match cli::parse(std::env::args_os().skip(1)) {
        Ok(command) => command,
        Err(error) => {
            let _ = writeln!(std::io::stderr(), "sheprd-automate: {error}");
            return ExitCode::from(2);
        }
    };
    let mut out = std::io::stdout().lock();
    match cli::run(&command, &mut out) {
        Ok(code) => code,
        // A closed pipe (`status | head`) is not the command's failure.
        Err(error) if error.kind() == std::io::ErrorKind::BrokenPipe => ExitCode::SUCCESS,
        Err(_) => ExitCode::FAILURE,
    }
}
