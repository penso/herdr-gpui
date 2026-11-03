//! What goes wrong asking a VS Code server whether it is there, or starting
//! the one the app runs. No message names the connection token: addresses
//! are named by host and port alone.
use super::NoAnswer;
use std::{io, path::PathBuf, process::ExitStatus, time::Duration};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Only the host and port are named: the address's query may hold the
    /// server's connection token.
    #[error("{reason} at {address}.")]
    Unreachable {
        address: String,
        reason: NoAnswer,
        #[source]
        source: ureq::Error,
    },
    #[error("This address is not a VS Code server (HTTP {status}).")]
    NotServer { status: u16 },
    #[error("The server refused the connection token. Use the address it prints, with its ?tkn=.")]
    TokenRefused,
    #[error("The VS Code server answered HTTP {0}.")]
    Status(u16),
    #[error("There is no folder for the app's data. Set HOME.")]
    NoDataDir,
    #[error("Could not use the VS Code token file {}.", .path.display())]
    TokenFile {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("Could not make a VS Code connection token.")]
    TokenRandom(#[source] getrandom::Error),
    #[error("Could not find a free port for VS Code.")]
    NoFreePort(#[source] io::Error),
    #[error(
        "Port {port} on 127.0.0.1 is in use by another program. Quit it, or remove `port` \
         under [code] in config-gpui.local.toml to pick another one; VS Code then starts \
         without the settings it kept for this port."
    )]
    PortTaken { port: u16 },
    #[error("Could not check port {port} on 127.0.0.1.")]
    PortCheck {
        port: u16,
        #[source]
        source: io::Error,
    },
    #[error("Could not start {}.", .program.display())]
    Spawn {
        program: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("VS Code stopped ({0}).")]
    Exited(ExitStatus),
    #[error("Could not watch the VS Code process.")]
    Watch(#[source] io::Error),
    #[error("VS Code did not answer within {} seconds.", .0.as_secs())]
    Silent(Duration),
    #[error("Could not start the VS Code worker.")]
    Worker(#[source] io::Error),
}

pub(super) type Result<T> = std::result::Result<T, Error>;
