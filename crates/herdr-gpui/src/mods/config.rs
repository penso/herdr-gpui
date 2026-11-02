//! `[mods]`: whether mods run, and which are turned off.

use serde::Deserialize;

const MAX_DISABLED: usize = 64;

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    /// Runs the mods in the mods folder.
    pub enabled: bool,
    /// Mods, by folder name, not to run.
    pub disabled: Vec<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            enabled: true,
            disabled: Vec::new(),
        }
    }
}

impl Config {
    pub(crate) fn validate(&self) -> crate::Result<()> {
        if self.disabled.len() > MAX_DISABLED {
            return Err(super::Error::TooManyDisabled(MAX_DISABLED).into());
        }
        if let Some(name) = self
            .disabled
            .iter()
            .find(|name| name.is_empty() || name.len() > 40 || name.chars().any(char::is_control))
        {
            return Err(super::Error::DisabledName(name.chars().take(40).collect()).into());
        }
        Ok(())
    }
}
