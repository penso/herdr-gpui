//! `[devices.<profile-id>]`: settings that differ per saved device, and the
//! targeted edits that write them.
use super::Error;
use super::{Config, LOCAL_CONFIG, ThemeName, write_config};
use crate::Result;
use serde::Deserialize;
use std::{fs, io::ErrorKind, path::Path};

/// A device list larger than any real catalog is a config mistake.
pub(super) const MAX_DEVICES: usize = 256;

/// Whose `[keys]` a saved device answers to, as `herdr --remote-keybindings`
/// chooses for the TUI. Local is upstream's default: muscle memory stays the
/// same on every host. Only keybindings follow the server; the app theme,
/// sidebar, and toasts stay local either way.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum KeybindingSource {
    #[default]
    Local,
    /// The host's published `server_keybindings_toml`.
    Server,
}

/// One `[devices.<profile-id>]` table.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub(crate) struct DeviceSettings {
    pub(crate) keybindings: KeybindingSource,
    /// The terminal's colors while this device is selected, as any top-level
    /// `theme` value. The sidebar, titlebar, and menus keep the app theme, so
    /// only the terminal tells devices apart. Unset, it is the app theme.
    pub(crate) theme: Option<String>,
}

impl DeviceSettings {
    /// Refuses a theme that could never load, naming the device. Whether a
    /// named file exists is only known once it is read, off the UI thread.
    pub(super) fn validate(&self, device: &str) -> Result<()> {
        let Some(theme) = &self.theme else {
            return Ok(());
        };
        let wrap = |source| Error::DeviceTheme {
            device: device.to_owned(),
            source: Box::new(source),
        };
        if theme.trim().is_empty() {
            return Err(wrap(Error::EmptyTheme.into()).into());
        }
        ThemeName::parse(theme)
            .map(drop)
            .map_err(|source| wrap(source).into())
    }
}

impl Config {
    /// The saved device an endpoint is, if any. Local and explicit sockets
    /// are not saved devices, so they always use the app's settings.
    fn device(&self, endpoint_id: &str) -> Option<&DeviceSettings> {
        crate::endpoint::saved_profile_id(endpoint_id).and_then(|profile| self.devices.get(profile))
    }

    /// Whose keybindings the endpoint uses.
    pub(crate) fn keybinding_source(&self, endpoint_id: &str) -> KeybindingSource {
        self.device(endpoint_id)
            .map(|device| device.keybindings)
            .unwrap_or_default()
    }

    /// The terminal theme the endpoint's device sets, or `None` for the app
    /// theme.
    pub(crate) fn device_theme(&self, endpoint_id: &str) -> Option<&str> {
        self.device(endpoint_id)?.theme.as_deref()
    }

    /// Every device's terminal theme, by profile ID.
    pub(crate) fn device_themes(&self) -> impl Iterator<Item = (&str, &str)> {
        self.devices
            .iter()
            .filter_map(|(profile, device)| Some((profile.as_str(), device.theme.as_deref()?)))
    }

    /// Persist one device's keybinding source, keeping the rest of the local
    /// file. Local is the default, so choosing it removes the entry.
    pub(crate) fn save_device_keybindings(profile: &str, source: KeybindingSource) -> Result<()> {
        let (_lock, local) = Self::prepare_files(&Self::path()?)?;
        Self::save_device_keybindings_path(profile, source, &local)
    }

    pub(super) fn save_device_keybindings_path(
        profile: &str,
        source: KeybindingSource,
        path: &Path,
    ) -> Result<()> {
        let value = match source {
            KeybindingSource::Server => Some(toml_edit::value("server")),
            KeybindingSource::Local => None,
        };
        set_device_key(profile, "keybindings", value, path)
    }

    /// Saves the device's terminal theme, or removes it with `None` so the
    /// device follows the app theme again.
    pub(crate) fn save_device_theme(profile: &str, theme: Option<&str>) -> Result<()> {
        let (_lock, local) = Self::prepare_files(&Self::path()?)?;
        Self::save_device_theme_path(profile, theme, &local)
    }

    pub(super) fn save_device_theme_path(
        profile: &str,
        theme: Option<&str>,
        path: &Path,
    ) -> Result<()> {
        let settings = DeviceSettings {
            theme: theme.map(str::to_owned),
            ..DeviceSettings::default()
        };
        settings.validate(profile)?;
        set_device_key(profile, "theme", theme.map(toml_edit::value), path)
    }
}

/// Sets one key of `[devices.<profile>]` in the local file, or removes it
/// with `None`, dropping a device table, and then `[devices]`, left empty.
/// Comments and every other key survive.
fn set_device_key(
    profile: &str,
    key: &str,
    value: Option<toml_edit::Item>,
    path: &Path,
) -> Result<()> {
    if !herdr_client::valid_profile_id(profile) {
        return Err(Error::InvalidDeviceId(profile.to_owned()).into());
    }
    let result = (|| -> Result<()> {
        let text = match fs::read_to_string(path) {
            Ok(text) => text,
            Err(error) if error.kind() == ErrorKind::NotFound => LOCAL_CONFIG.into(),
            Err(error) => return Err(error.into()),
        };
        let mut document = text.parse::<toml_edit::DocumentMut>()?;
        match value {
            Some(value) => {
                let mut devices = toml_edit::Table::new();
                devices.set_implicit(true);
                let device = document
                    .entry("devices")
                    .or_insert(toml_edit::Item::Table(devices))
                    .as_table_like_mut()
                    .ok_or(Error::InvalidDevicesTable)?
                    .entry(profile)
                    .or_insert(toml_edit::Item::Table(toml_edit::Table::new()))
                    .as_table_like_mut()
                    .ok_or(Error::InvalidDevicesTable)?;
                device.insert(key, value);
            }
            None => {
                let Some(devices) = document
                    .get_mut("devices")
                    .and_then(toml_edit::Item::as_table_like_mut)
                else {
                    return Ok(());
                };
                if let Some(device) = devices
                    .get_mut(profile)
                    .and_then(toml_edit::Item::as_table_like_mut)
                {
                    device.remove(key);
                    if device.is_empty() {
                        devices.remove(profile);
                    }
                }
                if devices.is_empty() {
                    document.remove("devices");
                }
            }
        }
        write_config(path, &document.to_string())
    })();
    result.map_err(|error| error.at_path(path))
}
