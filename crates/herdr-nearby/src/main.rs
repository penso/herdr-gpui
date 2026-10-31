//! `herdr-nearby` on Linux and Windows: Herdr UI's Bluetooth side of nearby
//! pairing and presence for Herdr Remote (`docs/ios-app-plan.md`, "Bluetooth
//! on the desktop"). Herdr UI runs it beside itself and speaks the same
//! newline-delimited JSON on stdio as the macOS helper
//! (`macos/herdr-nearby/main.swift`), and it serves the same GATT service,
//! so neither Herdr UI nor the phone tells them apart. It holds no secrets
//! and decides nothing: the code exchange and the presence proof run in the
//! daemon, which also advertises Bonjour on these platforms.
//!
//! `--no-bluetooth` leaves the radio alone, for tests; `--version` and
//! `--help` print and exit, for release smoke tests.

// Elsewhere the helper only reports Bluetooth unsupported, so the parts the
// radio uses are compiled (and tested) without a caller.
#![cfg_attr(not(any(target_os = "linux", windows)), allow(dead_code))]

mod backend;
mod error;
mod framing;
mod peripheral;
mod protocol;

pub use error::{Error, Result};

use peripheral::{Core, Shared};
use protocol::{Command, Event, Out};
use std::{
    process::ExitCode,
    sync::{Arc, Mutex, mpsc},
    thread,
};

const USAGE: &str = "\
Usage: herdr-nearby [--no-bluetooth]

Herdr UI's Bluetooth helper for nearby pairing and presence with Herdr
Remote. Herdr UI runs it and speaks newline-delimited JSON on stdio; it is
not meant to be run by hand.
";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|arg| arg == "-V" || arg == "--version") {
        println!(
            "herdr-nearby {}",
            option_env!("HERDR_RELEASE_VERSION").unwrap_or(env!("CARGO_PKG_VERSION"))
        );
        return ExitCode::SUCCESS;
    }
    if args.iter().any(|arg| arg == "-h" || arg == "--help") {
        print!("{USAGE}");
        return ExitCode::SUCCESS;
    }
    let bluetooth = !args.iter().any(|arg| arg == "--no-bluetooth");
    let out = Out;
    let core: Shared = Arc::new(Mutex::new(Core::new(Core::random_seed())));
    let (commands, received) = mpsc::channel();
    // Herdr UI's lines, read off the loop; the end of input means Herdr UI
    // went away, and the loop ends with the channel.
    thread::spawn(move || {
        let mut input = std::io::stdin().lock();
        let mut line = Vec::new();
        while let Ok(true) = protocol::read_line(&mut input, &mut line) {
            let text = String::from_utf8_lossy(&line);
            if let Some(command) = Command::parse(text.trim_end())
                && commands.send(command).is_err()
            {
                return;
            }
        }
    });
    if let Err(error) = backend::run(bluetooth, &core, &received, &out) {
        out.emit(&Event::error(&error));
        // The commands still have to be read until Herdr UI closes them,
        // so it never blocks writing to a helper that stopped listening.
        while received.recv().is_ok() {}
    }
    ExitCode::SUCCESS
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests;
