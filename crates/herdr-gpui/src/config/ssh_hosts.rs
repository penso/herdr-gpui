//! `[ssh]`: SSH hosts the user chose to connect to without verifying their
//! host keys, by the target saved with the device. The app pushes the list to
//! `herdr-client`, whose `ssh` children then accept any key those hosts offer.
use super::{Config, LOCAL_CONFIG, write_config};
use crate::Result;
use herdr_client::{HostKeys, MAX_UNVERIFIED_HOSTS};
use serde::Deserialize;
use std::{collections::BTreeSet, fs, io::ErrorKind, path::Path};

mod error;
pub use error::Error;

const TABLE: &str = "ssh";
const KEY: &str = "skip_host_key_verification";

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct SshConfig {
    /// Targets whose host keys are never checked. Anyone able to intercept
    /// the connection to one can impersonate it.
    pub skip_host_key_verification: BTreeSet<String>,
}

impl SshConfig {
    pub(super) fn check(&self) -> Result<()> {
        if self.skip_host_key_verification.len() > MAX_UNVERIFIED_HOSTS {
            return Err(Error::TooMany(MAX_UNVERIFIED_HOSTS).into());
        }
        if let Some(target) = self
            .skip_host_key_verification
            .iter()
            .find(|target| !valid_target(target))
        {
            return Err(Error::InvalidTarget(target.clone()).into());
        }
        Ok(())
    }
}

/// The same shape the Add Device form accepts.
fn valid_target(target: &str) -> bool {
    !target.is_empty()
        && target.len() <= 1024
        && !target.starts_with('-')
        && !target.chars().any(|c| c.is_whitespace() || c.is_control())
}

impl Config {
    /// Save whether `target`'s host keys are verified, keeping the rest of the
    /// local file, and apply it to this process's connections at once.
    /// Verifying is the default, so choosing it removes the target.
    pub(crate) fn save_host_keys(target: &str, keys: HostKeys) -> Result<()> {
        let (_lock, local) = Self::prepare_files(&Self::path()?)?;
        save_host_keys_path(target, keys, &local)?;
        herdr_client::set_host_keys(target, keys);
        Ok(())
    }
}

pub(super) fn save_host_keys_path(target: &str, keys: HostKeys, path: &Path) -> Result<()> {
    if !valid_target(target) {
        return Err(Error::InvalidTarget(target.to_owned()).into());
    }
    let result = (|| -> Result<()> {
        let text = match fs::read_to_string(path) {
            Ok(text) => text,
            Err(error) if error.kind() == ErrorKind::NotFound => LOCAL_CONFIG.into(),
            Err(error) => return Err(error.into()),
        };
        let mut document = text.parse::<toml_edit::DocumentMut>()?;
        let listed = |document: &toml_edit::DocumentMut| {
            document
                .get(TABLE)
                .and_then(|table| table.get(KEY))
                .and_then(toml_edit::Item::as_array)
                .is_some_and(|array| array.iter().any(|value| value.as_str() == Some(target)))
        };
        // Nothing to change leaves the file, and its modification time, alone.
        if listed(&document) == (keys == HostKeys::Skip) {
            return Ok(());
        }
        match keys {
            HostKeys::Skip => {
                let ssh = document
                    .entry(TABLE)
                    .or_insert(toml_edit::table())
                    .as_table_like_mut()
                    .ok_or(Error::InvalidTable)?;
                let array = ssh
                    .entry(KEY)
                    .or_insert(toml_edit::value(toml_edit::Array::new()))
                    .as_array_mut()
                    .ok_or(Error::InvalidTable)?;
                if array.len() >= MAX_UNVERIFIED_HOSTS {
                    return Err(Error::TooMany(MAX_UNVERIFIED_HOSTS).into());
                }
                array.push(target);
            }
            HostKeys::Verify => {
                let ssh = document
                    .get_mut(TABLE)
                    .and_then(toml_edit::Item::as_table_like_mut)
                    .ok_or(Error::InvalidTable)?;
                if let Some(array) = ssh.get_mut(KEY).and_then(toml_edit::Item::as_array_mut) {
                    array.retain(|value| value.as_str() != Some(target));
                    if array.is_empty() {
                        ssh.remove(KEY);
                    }
                }
                if ssh.is_empty() {
                    document.remove(TABLE);
                }
            }
        }
        write_config(path, &document.to_string())
    })();
    result.map_err(|error| error.at_path(path))
}

/// Keeps `herdr-client`'s list in step with the settings: now, from what the
/// app loaded at startup, and after every edit of the settings files.
pub(crate) fn install(startup: &SshConfig, cx: &mut gpui::App) {
    apply(cx, startup.clone());
    super::watch::follow(cx, "ssh", |config| config.ssh, apply).detach();
}

fn apply(_: &mut gpui::App, ssh: SshConfig) {
    herdr_client::set_unverified_hosts(ssh.skip_host_key_verification);
}
