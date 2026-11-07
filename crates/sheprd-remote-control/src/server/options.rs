//! Command-line options, parsed from OS strings before any runtime starts.

use crate::{app::config::ActivityDetail, push::Privacy};
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
    /// Whether to advertise over Bonjour, for phones nearby to find it.
    pub bonjour: bool,
    pub certificate: Certificate,
    /// The folder chat sockets live in, as Sheprd resolved it; `None`
    /// resolves it from this process's environment.
    pub chat_dir: Option<PathBuf>,
    /// The desktop endpoint, when explicitly passed by its service plan.
    pub core_endpoint: crate::core_target::CoreTarget,
    /// How long a device may stay away before it is forgotten; `None`
    /// keeps it until revoked.
    pub device_idle: Option<Duration>,
    /// The Herdr daemon's client socket, as Sheprd resolved it; `None`
    /// discovers it from this process's environment. A service manager's
    /// environment may lack the shell's `XDG_CONFIG_HOME`.
    pub herdr_socket: Option<PathBuf>,
    pub live_activities: ActivityDetail,
    pub port: u16,
    pub privacy: Privacy,
    pub state_dir: PathBuf,
    /// The saved WSL distributions, as Sheprd resolved them; `None` resolves
    /// them from this process's environment.
    pub wsl_catalog: Option<PathBuf>,
}

#[derive(Debug, thiserror::Error, Eq, PartialEq)]
pub enum ParseError {
    // keep-sorted start
    #[error("--cert and --key go together")]
    CertificatePair,
    #[error("--core-socket and --core-session go together")]
    CorePair,
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
    Serve(Box<Options>),
    /// Adds a phone another computer's Sheprd allowed, read as JSON from
    /// standard input (`app::enroll`).
    Enroll {
        state_dir: Option<PathBuf>,
    },
    /// Revokes a device that other computer enrolled.
    Revoke {
        state_dir: Option<PathBuf>,
        device: String,
    },
    /// Reads this computer's tags, offering any given on standard input.
    Tags {
        state_dir: Option<PathBuf>,
        offer: bool,
    },
    Help,
    Version,
}

pub const USAGE: &str = "\
Usage: sheprd-remote-control serve --state-dir DIR [options]
       sheprd-remote-control enroll [--state-dir DIR] < REQUEST.json
       sheprd-remote-control revoke --device ID [--state-dir DIR]
       sheprd-remote-control tags [--offer] [--state-dir DIR] [< TAGS.json]

Serves this computer's Herdr chats to paired phones over HTTPS. Sheprd
starts it when Remote control is on.

Options:
  --state-dir DIR          Where keys, devices, and certificates live (required)
  --port N                 HTTPS port (default 7443)
  --bind all|ADDRESS       Listen on every interface (default) or one address
  --cert FILE --key FILE   Use this certificate chain and key instead of the
                           daemon's own certificate authority
  --herdr-socket PATH      The Herdr daemon's client socket (default: discovered);
                           other running Herdr sessions are followed too
  --core-socket PATH --core-session NAME
                           The desktop core endpoint (default: discovered)
  --chat-dir DIR           Where chat sockets live (default: discovered)
  --wsl-catalog FILE       The saved WSL distributions (default: discovered)
  --privacy generic|excerpt
                           Whether notifications may quote the chat (default excerpt)
  --live-activities detailed|generic
                           Whether Live Activities name the chat and its step
                           (default detailed)
  --device-idle-days N     Forget devices unseen this long, 0 for never
                           (default 30)
  --no-bonjour             Do not advertise over Bonjour; phones then pair
                           by QR code only (on macOS, Sheprd advertises
                           while its pairing dialog is open instead)
  -h, --help               Show this help
  -V, --version            Show the version

enroll and revoke are what Sheprd on another computer runs here over SSH,
so a phone paired there also reaches this computer. enroll starts the
daemon when it does not run and prints this computer's answer as JSON.
Their state directory defaults to the one Sheprd on this computer uses.
";

pub fn parse(args: impl IntoIterator<Item = OsString>) -> Result<Invocation, ParseError> {
    let mut args = args.into_iter().skip(1);
    match args.next().as_deref().and_then(|arg| arg.to_str()) {
        Some("serve") => {}
        Some(verb @ ("enroll" | "revoke")) => return parse_host(verb == "enroll", args),
        Some("tags") => return parse_tags(args),
        Some("-h" | "--help") | None => return Ok(Invocation::Help),
        Some("-V" | "--version") => return Ok(Invocation::Version),
        Some(other) => return Err(ParseError::Unknown(other.to_owned())),
    }
    let mut state_dir = None;
    let mut port = DEFAULT_PORT;
    let mut bind = IpAddr::V6(Ipv6Addr::UNSPECIFIED);
    let (mut cert, mut key) = (None, None);
    let mut privacy = Privacy::default();
    let mut live_activities = ActivityDetail::default();
    let mut idle_days = DEFAULT_IDLE_DAYS;
    let (mut herdr_socket, mut chat_dir, mut wsl_catalog) = (None, None, None);
    let (mut core_socket, mut core_session) = (None, None);
    let mut bonjour = true;
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
            "--core-socket" => core_socket = Some(PathBuf::from(value("--core-socket")?)),
            "--core-session" => {
                core_session = Some(text(value("--core-session")?, "--core-session")?)
            }
            "--chat-dir" => chat_dir = Some(PathBuf::from(value("--chat-dir")?)),
            "--wsl-catalog" => wsl_catalog = Some(PathBuf::from(value("--wsl-catalog")?)),
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
            "--live-activities" => {
                live_activities =
                    match text(value("--live-activities")?, "--live-activities")?.as_str() {
                        "detailed" => ActivityDetail::Detailed,
                        "generic" => ActivityDetail::Generic,
                        _ => {
                            return Err(ParseError::Value {
                                option: "--live-activities",
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
            "--no-bonjour" => bonjour = false,
            "-h" | "--help" => return Ok(Invocation::Help),
            _ => return Err(ParseError::Unknown(name)),
        }
    }
    let certificate = match (cert, key) {
        (None, None) => Certificate::Own,
        (Some(cert), Some(key)) => Certificate::Files { cert, key },
        _ => return Err(ParseError::CertificatePair),
    };
    let core_endpoint = match (core_socket, core_session) {
        (None, None) => crate::core_target::CoreTarget::Discover,
        (Some(socket), Some(session)) => {
            crate::core_target::CoreTarget::Endpoint(sheprd_core_client::Endpoint {
                socket,
                session,
            })
        }
        _ => return Err(ParseError::CorePair),
    };
    Ok(Invocation::Serve(Box::new(Options {
        bind,
        bonjour,
        certificate,
        chat_dir,
        core_endpoint,
        device_idle: (idle_days != 0).then(|| Duration::from_secs(idle_days * 24 * 60 * 60)),
        herdr_socket,
        live_activities,
        port,
        privacy,
        state_dir: state_dir.ok_or(ParseError::NoStateDir)?,
        wsl_catalog,
    })))
}

/// `enroll [--state-dir DIR]` and `revoke --device ID [--state-dir DIR]`.
fn parse_host(
    enroll: bool,
    mut args: impl Iterator<Item = OsString>,
) -> Result<Invocation, ParseError> {
    let (mut state_dir, mut device) = (None, None);
    while let Some(arg) = args.next() {
        let name = arg.to_str().unwrap_or_default().to_owned();
        match name.as_str() {
            "--state-dir" => {
                state_dir = Some(PathBuf::from(
                    args.next().ok_or(ParseError::Missing("--state-dir"))?,
                ));
            }
            "--device" if !enroll => {
                device = Some(
                    args.next()
                        .ok_or(ParseError::Missing("--device"))?
                        .into_string()
                        .ok()
                        .filter(|device| !device.is_empty())
                        .ok_or(ParseError::Value { option: "--device" })?,
                );
            }
            "-h" | "--help" => return Ok(Invocation::Help),
            _ => return Err(ParseError::Unknown(name)),
        }
    }
    if enroll {
        return Ok(Invocation::Enroll { state_dir });
    }
    Ok(Invocation::Revoke {
        state_dir,
        device: device.ok_or(ParseError::Missing("--device"))?,
    })
}

/// `tags [--offer] [--state-dir DIR]`.
fn parse_tags(mut args: impl Iterator<Item = OsString>) -> Result<Invocation, ParseError> {
    let (mut state_dir, mut offer) = (None, false);
    while let Some(arg) = args.next() {
        let name = arg.to_str().unwrap_or_default().to_owned();
        match name.as_str() {
            "--state-dir" => {
                state_dir = Some(PathBuf::from(
                    args.next().ok_or(ParseError::Missing("--state-dir"))?,
                ));
            }
            "--offer" => offer = true,
            "-h" | "--help" => return Ok(Invocation::Help),
            _ => return Err(ParseError::Unknown(name)),
        }
    }
    Ok(Invocation::Tags { state_dir, offer })
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
