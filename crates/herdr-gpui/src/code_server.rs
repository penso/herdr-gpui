//! Asking a `code serve-web` server whether it is there. A page that cannot
//! load reports nothing back through the web view, so the VS Code tab and
//! its settings page probe the server over plain HTTP first. Blocking: run it
//! off the UI thread.
//!
//! The submodules start such a server for VS Code tabs: [`cli`] finds VS Code's
//! command, [`token`] keeps the connection token, [`supervisor`] runs the
//! child process, and [`launcher`] ties them to the windows and the config.
mod cli;
mod error;
pub(crate) mod launcher;
pub(crate) mod supervisor;
mod token;

pub use error::Error;
pub(crate) use launcher::{Launcher, Startup};

use crate::{Result, browser::WebUrl};
use std::{fmt, io::Read, time::Duration};

/// How long each request may take.
const TIMEOUT: Duration = Duration::from_secs(3);
/// More than a commit hash and a line break.
const VERSION_BYTES: u64 = 128;

/// A server that answered as VS Code does.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Server {
    /// The commit of the VS Code build it serves: 40 lowercase hex digits.
    commit: String,
}

impl Server {
    /// The server behind a `/version` body, which is its build's commit.
    pub(crate) fn from_version(body: &str) -> Option<Self> {
        let commit = body.trim();
        let hex = |byte: u8| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte);
        (commit.len() == 40 && commit.bytes().all(hex)).then(|| Self {
            commit: commit.to_owned(),
        })
    }

    /// The commit, shortened as git shows it.
    pub(crate) fn short_commit(&self) -> &str {
        &self.commit[..7]
    }
}

/// Asks the server at `url` for its version, then checks that it takes the
/// address's connection token. `/version` needs no token, so a server that
/// answers it but refuses the address has the wrong one.
pub(crate) fn probe(url: &WebUrl) -> Result<Server> {
    let (agent, unreachable) = (agent(), unreachable(url));
    let server = ask_version(&agent, url)?;
    // The token is accepted with a redirect that sets its cookie.
    match agent
        .get(url.as_str())
        .call()
        .map_err(unreachable)?
        .status()
        .as_u16()
    {
        200..=399 => Ok(server),
        401 | 403 => Err(Error::TokenRefused.into()),
        status => Err(Error::Status(status).into()),
    }
}

/// Asks the server at `url` for its version alone, sending nothing of the
/// address's token: for the server the app started, whose token it wrote,
/// so that no question carries the token to whatever answers the port.
pub(crate) fn version(url: &WebUrl) -> Result<Server> {
    ask_version(&agent(), url)
}

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(TIMEOUT))
        .max_redirects(0)
        .http_status_as_error(false)
        .build()
        .into()
}

fn unreachable(url: &WebUrl) -> impl Fn(ureq::Error) -> Error + '_ {
    move |source| Error::Unreachable {
        address: url.address(),
        reason: NoAnswer::from(&source),
        source,
    }
}

fn ask_version(agent: &ureq::Agent, url: &WebUrl) -> Result<Server> {
    let mut response = agent
        .get(format!("{}/version", url.origin()))
        .call()
        .map_err(unreachable(url))?;
    let status = response.status().as_u16();
    let mut body = String::new();
    let read = response
        .body_mut()
        .as_reader()
        .take(VERSION_BYTES)
        .read_to_string(&mut body);
    Ok(Some(status)
        .filter(|status| *status == 200 && read.is_ok())
        .and_then(|_| Server::from_version(&body))
        .ok_or(Error::NotServer { status })?)
}

/// Why a request got no answer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NoAnswer {
    Refused,
    Timeout,
    UnknownHost,
    Other,
}

impl From<&ureq::Error> for NoAnswer {
    fn from(error: &ureq::Error) -> Self {
        match error {
            ureq::Error::Io(error) if error.kind() == std::io::ErrorKind::ConnectionRefused => {
                Self::Refused
            }
            ureq::Error::ConnectionFailed => Self::Refused,
            ureq::Error::Timeout(_) => Self::Timeout,
            ureq::Error::HostNotFound => Self::UnknownHost,
            _ => Self::Other,
        }
    }
}

impl fmt::Display for NoAnswer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Refused => f.write_str("Connection refused"),
            Self::Timeout => write!(f, "No answer within {} seconds", TIMEOUT.as_secs()),
            Self::UnknownHost => f.write_str("Unknown host"),
            Self::Other => f.write_str("No connection"),
        }
    }
}

#[cfg(test)]
mod tests;
