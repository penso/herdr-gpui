//! The relay's command line. Secrets are files in one directory, mounted
//! read-only, so none of them reach the process's arguments or environment.

use crate::{Error, Result};
use std::{
    ffi::{OsStr, OsString},
    net::SocketAddr,
    path::PathBuf,
};

pub const USAGE: &str = "\
sheprd-push-relay: forwards sealed pushes to APNs for the iOS app.

Usage: sheprd-push-relay --secrets DIR [--store STORE] [--listen ADDRESS] [--behind-proxy]

  --secrets DIR      Holds apns/config.json, apns/AuthKey.p8, relay-key.pem,
                     optionally relay-key-previous.pem, and credential-secret
                     (32 or more bytes as hex). All private to this user.
  --store STORE      Where revocations live: memory (default), sqlite:FILE, or
                     postgres, which reads postgres-url from the secrets and
                     trusts postgres-ca.pem there if there is one. Pushes keep
                     working through a restart in any of them; memory forgets
                     revocations, which then lapse with their credentials.
  --listen ADDRESS   Where to listen (default 127.0.0.1:8080).
  --behind-proxy     Take the caller's address from X-Forwarded-For. Use only
                     when nothing can reach the relay except that proxy.
  --help             Show this.";

const DEFAULT_LISTEN: ([u8; 4], u16) = ([127, 0, 0, 1], 8080);

/// Which store to run, as `--store` names it.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub enum Backend {
    // keep-sorted start
    #[default]
    Memory,
    Postgres,
    Sqlite(PathBuf),
    // keep-sorted end
}

impl Backend {
    fn parse(text: &OsStr) -> Option<Self> {
        let text = text.to_str()?;
        match text.split_once(':') {
            None if text == "memory" => Some(Self::Memory),
            None if text == "postgres" => Some(Self::Postgres),
            Some(("sqlite", path)) if !path.is_empty() => Some(Self::Sqlite(path.into())),
            _ => None,
        }
    }
}

/// What the relay was started with.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Options {
    pub backend: Backend,
    pub behind_proxy: bool,
    pub listen: SocketAddr,
    pub secrets: PathBuf,
}

impl Options {
    /// The options in `arguments`, or `None` when they ask for help.
    pub fn parse(arguments: impl IntoIterator<Item = OsString>) -> Result<Option<Self>> {
        let mut arguments = arguments.into_iter();
        let mut backend = Backend::default();
        let mut behind_proxy = false;
        let mut listen = SocketAddr::from(DEFAULT_LISTEN);
        let mut secrets = None;
        while let Some(argument) = arguments.next() {
            let mut value = || {
                arguments.next().ok_or_else(|| Error::Argument {
                    argument: argument.clone(),
                })
            };
            match argument.to_str() {
                Some("--behind-proxy") => behind_proxy = true,
                Some("--help") => return Ok(None),
                Some("--listen") => {
                    let text = value()?;
                    listen = text
                        .to_str()
                        .and_then(|text| text.parse().ok())
                        .ok_or(Error::Argument { argument: text })?;
                }
                Some("--secrets") => secrets = Some(PathBuf::from(value()?)),
                Some("--store") => {
                    let text = value()?;
                    backend = Backend::parse(&text).ok_or(Error::Argument { argument: text })?;
                }
                _ => return Err(Error::Argument { argument }),
            }
        }
        let missing = |name: &str| Error::Argument {
            argument: name.into(),
        };
        Ok(Some(Self {
            backend,
            behind_proxy,
            listen,
            secrets: secrets.ok_or_else(|| missing("--secrets"))?,
        }))
    }
}
