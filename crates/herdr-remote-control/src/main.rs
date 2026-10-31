//! `herdr-remote-control serve`: the remote control daemon. Herdr UI installs it as
//! a user service when Remote control is on; see `--help`.
// On Windows the logon task would otherwise open a console window for the
// life of the daemon. The cost: `--help` and errors print nowhere there.
#![cfg_attr(windows, windows_subsystem = "windows")]

use herdr_remote_control::{
    app::enroll,
    control::DeviceId,
    server::{self, Invocation},
};
use std::{io::Read, path::PathBuf, process::ExitCode};

/// The largest `enroll` request read: a key, names, and APNs files.
const MAX_INPUT: u64 = 64 * 1024;

fn main() -> ExitCode {
    let options = match server::parse(std::env::args_os()) {
        Ok(Invocation::Serve(options)) => options,
        Ok(Invocation::Enroll { state_dir }) => return host(state_dir, run_enroll),
        Ok(Invocation::Revoke { state_dir, device }) => {
            return host(state_dir, |state_dir| {
                enroll::revoke(state_dir, DeviceId(device))?;
                Ok(())
            });
        }
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
    // Standard error is the service's log file (`daemon.log`).
    tracing_subscriber::fmt()
        .with_ansi(false)
        .with_max_level(tracing::Level::INFO)
        .with_writer(std::io::stderr)
        .init();
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
        Err(error) => fail(&error),
    }
}

/// Runs `enroll` or `revoke` against `state_dir`, or the state directory
/// Herdr UI on this computer would use.
fn host(
    state_dir: Option<PathBuf>,
    run: impl FnOnce(&std::path::Path) -> herdr_remote_control::Result<()>,
) -> ExitCode {
    let Some(state_dir) = state_dir.or_else(enroll::default_state_dir) else {
        eprintln!("herdr-remote: this computer has no state directory; pass --state-dir");
        return ExitCode::from(2);
    };
    match run(&state_dir) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => fail(&error),
    }
}

fn run_enroll(state_dir: &std::path::Path) -> herdr_remote_control::Result<()> {
    let mut text = String::new();
    std::io::stdin().take(MAX_INPUT).read_to_string(&mut text)?;
    let input: enroll::Input = serde_json::from_str(&text).map_err(std::io::Error::from)?;
    let program = std::env::current_exe()?;
    let enrolled = enroll::enroll(&input, state_dir, program)?;
    println!(
        "{}",
        serde_json::to_string(&enrolled).map_err(std::io::Error::from)?
    );
    Ok(())
}

/// Prints `error` with its causes and fails.
fn fail(error: &dyn std::error::Error) -> ExitCode {
    let mut message = error.to_string();
    let mut source = error.source();
    while let Some(cause) = source {
        message.push_str(&format!(": {cause}"));
        source = cause.source();
    }
    eprintln!("herdr-remote: {message}");
    ExitCode::FAILURE
}
