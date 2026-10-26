//! Command-line options, parsed from OS strings before any runtime starts.

use crate::push::Privacy;
use std::{
    ffi::OsString,
    net::{IpAddr, Ipv6Addr},
    path::PathBuf,
    time::Duration,
};

pub const DEFAULT_PORT: u16 = 7443;
const DEFAULT_IDLE_DAYS: u64 = 30;
const MAX_IDLE_DAYS: u64 = 3650;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Certificate {
    /// The daemon's own certificate authority.
    Own,
    /// A certificate chain and key the user provides, as PEM files.
    Files { cert: PathBuf, key: PathBuf },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Options {
    /// Where to listen: every interface (`[::]`, dual stack) or one address.
    pub bind: IpAddr,
    pub certificate: Certificate,
    /// The folder chat sockets live in, as Herdr UI resolved it; `None`
    /// resolves it from this process's environment.
    pub chat_dir: Option<PathBuf>,
    /// How long a device may stay away before it is forgotten; `None`
    /// keeps it until revoked.
    pub device_idle: Option<Duration>,
    /// The Herdr daemon's client socket, as Herdr UI resolved it; `None`
    /// discovers it from this process's environment. A service manager's
    /// environment may lack the shell's `XDG_CONFIG_HOME`.
    pub herdr_socket: Option<PathBuf>,
    pub port: u16,
    pub privacy: Privacy,
    pub state_dir: PathBuf,
}

#[derive(Debug, thiserror::Error, Eq, PartialEq)]
pub enum ParseError {
    // keep-sorted start
    #[error("--cert and --key go together")]
    CertificatePair,
    #[error("{0} needs a value")]
    Missing(&'static str),
    #[error("--state-dir is required")]
    NoStateDir,
    #[error("unknown option {0}")]
    Unknown(String),
    #[error("{option} has an invalid value")]
    Value { option: &'static str },
    // keep-sorted end
}

/// What the command line asks for.
#[derive(Debug, Eq, PartialEq)]
pub enum Invocation {
    Serve(Options),
    Help,
    Version,
}

pub const USAGE: &str = "\
Usage: herdr-remote-control serve --state-dir DIR [options]

Serves this computer's Herdr chats to paired phones over HTTPS. Herdr UI
starts it when Remote control is on.

Options:
  --state-dir DIR          Where keys, devices, and certificates live (required)
  --port N                 HTTPS port (default 7443)
  --bind all|ADDRESS       Listen on every interface (default) or one address
  --cert FILE --key FILE   Use this certificate chain and key instead of the
                           daemon's own certificate authority
  --herdr-socket PATH      The Herdr daemon's client socket (default: discovered)
  --chat-dir DIR           Where chat sockets live (default: discovered)
  --privacy generic|excerpt
                           Whether notifications may quote the chat (default generic)
  --device-idle-days N     Forget devices unseen this long, 0 for never
                           (default 30)
  -h, --help               Show this help
  -V, --version            Show the version
";

pub fn parse(args: impl IntoIterator<Item = OsString>) -> Result<Invocation, ParseError> {
    let mut args = args.into_iter().skip(1);
    match args.next().as_deref().and_then(|arg| arg.to_str()) {
        Some("serve") => {}
        Some("-h" | "--help") | None => return Ok(Invocation::Help),
        Some("-V" | "--version") => return Ok(Invocation::Version),
        Some(other) => return Err(ParseError::Unknown(other.to_owned())),
    }
    let mut state_dir = None;
    let mut port = DEFAULT_PORT;
    let mut bind = IpAddr::V6(Ipv6Addr::UNSPECIFIED);
    let (mut cert, mut key) = (None, None);
    let mut privacy = Privacy::Generic;
    let mut idle_days = DEFAULT_IDLE_DAYS;
    let (mut herdr_socket, mut chat_dir) = (None, None);
    while let Some(arg) = args.next() {
        let name = arg.to_str().unwrap_or_default().to_owned();
        let mut value = |option: &'static str| args.next().ok_or(ParseError::Missing(option));
        let text = |value: OsString, option: &'static str| {
            value
                .into_string()
                .map_err(|_| ParseError::Value { option })
        };
        match name.as_str() {
            "--state-dir" => state_dir = Some(PathBuf::from(value("--state-dir")?)),
            "--port" => {
                port = text(value("--port")?, "--port")?
                    .parse()
                    .ok()
                    .filter(|port| *port != 0)
                    .ok_or(ParseError::Value { option: "--port" })?;
            }
            "--bind" => {
                let address = text(value("--bind")?, "--bind")?;
                if address != "all" {
                    bind = address
                        .parse()
                        .map_err(|_| ParseError::Value { option: "--bind" })?;
                }
            }
            "--cert" => cert = Some(PathBuf::from(value("--cert")?)),
            "--key" => key = Some(PathBuf::from(value("--key")?)),
            "--herdr-socket" => herdr_socket = Some(PathBuf::from(value("--herdr-socket")?)),
            "--chat-dir" => chat_dir = Some(PathBuf::from(value("--chat-dir")?)),
            "--privacy" => {
                privacy = match text(value("--privacy")?, "--privacy")?.as_str() {
                    "generic" => Privacy::Generic,
                    "excerpt" => Privacy::Excerpt,
                    _ => {
                        return Err(ParseError::Value {
                            option: "--privacy",
                        });
                    }
                };
            }
            "--device-idle-days" => {
                idle_days = text(value("--device-idle-days")?, "--device-idle-days")?
                    .parse()
                    .ok()
                    .filter(|days| *days <= MAX_IDLE_DAYS)
                    .ok_or(ParseError::Value {
                        option: "--device-idle-days",
                    })?;
            }
            "-h" | "--help" => return Ok(Invocation::Help),
            _ => return Err(ParseError::Unknown(name)),
        }
    }
    let certificate = match (cert, key) {
        (None, None) => Certificate::Own,
        (Some(cert), Some(key)) => Certificate::Files { cert, key },
        _ => return Err(ParseError::CertificatePair),
    };
    Ok(Invocation::Serve(Options {
        bind,
        certificate,
        chat_dir,
        device_idle: (idle_days != 0).then(|| Duration::from_secs(idle_days * 24 * 60 * 60)),
        herdr_socket,
        port,
        privacy,
        state_dir: state_dir.ok_or(ParseError::NoStateDir)?,
    }))
}

/// The computer's name as its owner knows it.
pub(crate) fn computer_name() -> String {
    #[cfg(unix)]
    {
        let name = rustix::system::uname()
            .nodename()
            .to_string_lossy()
            .into_owned();
        let name = name.strip_suffix(".local").unwrap_or(&name).to_owned();
        if !name.is_empty() {
            return name;
        }
    }
    std::env::var("COMPUTERNAME").unwrap_or_else(|_| "This computer".to_owned())
}
