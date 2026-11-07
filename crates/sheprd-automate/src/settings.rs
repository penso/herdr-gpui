//! The host's own settings, from `config.toml` beside personal scripts:
//! caps for every script together, and which Herdr to drive.

use serde::Deserialize;
use sheprd_automation::report::HostCaps;
use std::{fs, io, path::Path};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{}: {source}", path.display())]
    Read {
        path: std::path::PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("{}: {source}", path.display())]
    Parse {
        path: std::path::PathBuf,
        #[source]
        source: toml::de::Error,
    },
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Settings {
    pub caps: HostCaps,
    /// The Herdr session to drive; Herdr's default when `None`.
    pub herdr_session: Option<String>,
    /// The `herdr` binary; found on the usual paths when `None`.
    pub herdr: Option<std::path::PathBuf>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct File {
    max_agents: Option<u32>,
    max_tokens_per_day: Option<u64>,
    herdr_session: Option<String>,
    herdr: Option<std::path::PathBuf>,
}

impl Settings {
    /// Reads `path`; a missing file means the defaults.
    pub fn read(path: &Path) -> Result<Self, Error> {
        let text = match fs::read_to_string(path) {
            Ok(text) => text,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(source) => {
                return Err(Error::Read {
                    path: path.to_owned(),
                    source,
                });
            }
        };
        let file = toml::from_str::<File>(&text).map_err(|source| Error::Parse {
            path: path.to_owned(),
            source,
        })?;
        let defaults = HostCaps::default();
        Ok(Self {
            caps: HostCaps {
                max_agents: file.max_agents.unwrap_or(defaults.max_agents),
                max_tokens_per_day: file
                    .max_tokens_per_day
                    .unwrap_or(defaults.max_tokens_per_day),
            },
            herdr_session: file.herdr_session.filter(|session| !session.is_empty()),
            herdr: file.herdr,
        })
    }
}
