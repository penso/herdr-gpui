//! The `[sprites]` table: a Fly Sprites account whose sprites become devices.
//! A build without the `sprites` feature still parses the table, so one
//! config file serves every build, but never reads it. The API token is never
//! read from here: Settings keeps it in the credential store.
#[cfg(feature = "sprites")]
use crate::Result;
use serde::Deserialize;
#[cfg(feature = "sprites")]
use std::{env, ffi::OsString};

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct SpritesConfig {
    // keep-sorted start
    pub allow_plaintext_credentials: bool,
    /// The API root, `https://api.sprites.dev` for Fly's service.
    pub api_url: Option<String>,
    // keep-sorted end
}

#[cfg(feature = "sprites")]
impl SpritesConfig {
    /// The validated account settings, or `None` when Sprites is not set up.
    /// `HERDR_SPRITES_API_URL` replaces `api_url`.
    pub fn settings(&self) -> Result<Option<crate::sprites::Settings>> {
        self.settings_with(|name| env::var_os(name))
    }

    pub fn settings_with(
        &self,
        var: impl Fn(&str) -> Option<OsString>,
    ) -> Result<Option<crate::sprites::Settings>> {
        Ok(crate::sprites::Settings::resolve(self, var)?)
    }
}

/// The `[sprites]` keys the Settings window edits. An empty value removes its
/// key.
#[cfg(feature = "sprites")]
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SpritesFields {
    pub api_url: String,
}

#[cfg(feature = "sprites")]
impl SpritesFields {
    pub fn from_config(config: &SpritesConfig) -> Self {
        Self {
            api_url: config.api_url.clone().unwrap_or_default(),
        }
    }

    pub fn entries(&self) -> [(&'static str, &str); 1] {
        [("api_url", &self.api_url)]
    }

    /// The checks loading applies, so a save cannot leave a config that then
    /// fails to load.
    pub fn validate(&self, existing: &SpritesConfig) -> Result<()> {
        let api_url = self.api_url.trim();
        SpritesConfig {
            api_url: (!api_url.is_empty()).then(|| api_url.to_owned()),
            ..existing.clone()
        }
        .settings_with(|_| None)
        .map(drop)
    }
}
