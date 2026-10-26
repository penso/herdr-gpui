//! `herdr-acp`: runs an ACP agent in a terminal pane, draws the conversation
//! there, and lets Herdr UI attach to it.

mod agents;
mod app;
mod attach;
mod claude;
mod cli;
mod codex;
mod error;
mod files;
mod harness;
mod herdr;
mod mirror;
mod observe;
mod pi;
mod pictures;
mod process;
mod project;
mod relay;
mod rpc;
mod session;
mod tui;

use std::process::ExitCode;

pub(crate) use error::{Error, Result};

fn main() -> ExitCode {
    match cli::parse(std::env::args_os().skip(1)) {
        Ok(cli::Parsed::Help) => {
            print!("{}", cli::HELP);
            ExitCode::SUCCESS
        }
        Ok(cli::Parsed::Version) => {
            println!("herdr-acp {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        Ok(cli::Parsed::Agents) => {
            let path = agents::search_path(
                std::env::var_os("PATH").as_deref(),
                std::env::home_dir().as_deref(),
            );
            for agent in agents::installed(Some(&path)) {
                println!("{agent}");
            }
            ExitCode::SUCCESS
        }
        Ok(cli::Parsed::Connect(id)) => match relay::run(&id) {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("herdr-acp: {error:#}");
                ExitCode::FAILURE
            }
        },
        Ok(cli::Parsed::Mirror(options)) => match mirror::run(options) {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("herdr-acp: {error:#}");
                ExitCode::FAILURE
            }
        },
        Ok(cli::Parsed::Observe(options)) => match observe::run(options) {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("herdr-acp: {error:#}");
                ExitCode::FAILURE
            }
        },
        Ok(cli::Parsed::Run(options)) => match app::run(options) {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("herdr-acp: {error:#}");
                ExitCode::FAILURE
            }
        },
        Err(error) => {
            eprintln!("herdr-acp: {error}\n\nRun `herdr-acp --help` for usage.");
            ExitCode::from(2)
        }
    }
}
