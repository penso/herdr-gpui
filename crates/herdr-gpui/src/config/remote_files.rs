//! `[remote_files]` size limits for copying a folder printed in an SSH pane.
use serde::Deserialize;

/// How large a remote folder may get before the app warns or refuses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RemoteFilesConfig {
    /// Warn once the folder's uncompressed contents exceed this many megabytes.
    pub folder_warning_mb: u32,
    /// Refuse an archive larger than this many megabytes.
    pub folder_transfer_cap_mb: u32,
}

impl Default for RemoteFilesConfig {
    fn default() -> Self {
        Self {
            folder_warning_mb: 64,
            folder_transfer_cap_mb: 512,
        }
    }
}

impl RemoteFilesConfig {
    /// The warning must not sit above the cap, or it could never fire.
    pub(crate) fn valid(&self) -> bool {
        self.folder_warning_mb <= self.folder_transfer_cap_mb
    }
}

/// Only explicitly configured keys override the defaults.
#[derive(Clone, Copy, Debug, Default, Deserialize)]
#[serde(default)]
pub(crate) struct RemoteFilesSettings {
    #[serde(deserialize_with = "optional_warning_mb")]
    folder_warning_mb: Option<u32>,
    #[serde(deserialize_with = "optional_transfer_cap_mb")]
    folder_transfer_cap_mb: Option<u32>,
}

impl RemoteFilesSettings {
    pub(super) fn resolve(self, base: RemoteFilesConfig) -> RemoteFilesConfig {
        RemoteFilesConfig {
            folder_warning_mb: self.folder_warning_mb.unwrap_or(base.folder_warning_mb),
            folder_transfer_cap_mb: self
                .folder_transfer_cap_mb
                .unwrap_or(base.folder_transfer_cap_mb),
        }
    }
}

fn optional_warning_mb<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<u32>, D::Error> {
    warning_mb(d).map(Some)
}

fn warning_mb<'de, D: serde::Deserializer<'de>>(d: D) -> Result<u32, D::Error> {
    let value = u32::deserialize(d)?;
    if !(1..=4096).contains(&value) {
        return Err(serde::de::Error::custom(
            "remote_files.folder_warning_mb must be between 1 and 4096",
        ));
    }
    Ok(value)
}

fn optional_transfer_cap_mb<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<Option<u32>, D::Error> {
    transfer_cap_mb(d).map(Some)
}

fn transfer_cap_mb<'de, D: serde::Deserializer<'de>>(d: D) -> Result<u32, D::Error> {
    let value = u32::deserialize(d)?;
    if !(1..=8192).contains(&value) {
        return Err(serde::de::Error::custom(
            "remote_files.folder_transfer_cap_mb must be between 1 and 8192",
        ));
    }
    Ok(value)
}
