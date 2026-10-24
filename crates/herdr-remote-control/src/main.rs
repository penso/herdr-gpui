//! `herdr-remote-control serve`: the remote control daemon. Herdr UI installs it as
//! a user service when Remote control is on; see `--help`.
// On Windows the logon task would otherwise open a console window for the
// life of the daemon. The cost: `--help` and errors print nowhere there.
#![cfg_attr(windows, windows_subsystem = "windows")]

use herdr_remote_control::server::{self, Invocation};
use std::process::ExitCode;

fn main() -> ExitCode {
    let options = match server::parse(std::env::args_os()) {
        Ok(Invocation::Serve(options)) => options,
        Ok(Invocation::Help) => {
            print!("{}", server::USAGE);
            return ExitCode::SUCCESS;
        }
        Ok(Invocation::Version) => {
            println!("herdr-remote-control {}", env!("CARGO_PKG_VERSION"));
            return ExitCode::SUCCESS;
        }
        Err(error) => {
            eprintln!("herdr-remote: {error}\n\n{}", server::USAGE);
            return ExitCode::from(2);
        }
    };
    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_name("herdr-remote-control")
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => {
            eprintln!("herdr-remote: could not start: {error}");
            return ExitCode::FAILURE;
        }
    };
    match runtime.block_on(server::run(options)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            let mut message = error.to_string();
            let mut source = std::error::Error::source(&error);
            while let Some(cause) = source {
                message.push_str(&format!(": {cause}"));
                source = cause.source();
            }
            eprintln!("herdr-remote: {message}");
            ExitCode::FAILURE
        }
    }
}
