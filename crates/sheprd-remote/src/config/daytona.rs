//! The `[daytona]` table: a Daytona account whose sandboxes become devices.
//! A build without the `daytona` feature still parses the table, so one
//! config file serves every build, but never reads it. The API key is never
//! read from here: Settings keeps it in the credential store.
#[cfg(feature = "daytona")]
use crate::Result;
use serde::Deserialize;
#[cfg(feature = "daytona")]
use std::{env, ffi::OsString};

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct DaytonaConfig {
    /// The API root, `https://app.daytona.io/api` for Daytona's cloud.
    pub api_url: Option<String>,
    pub organization_id: Option<String>,
    /// The region new sandboxes are created in; Daytona's default if unset.
    pub target: Option<String>,
    /// The snapshot new sandboxes start from; Daytona's default if unset.
    pub snapshot: Option<String>,
    pub allow_plaintext_credentials: bool,
}

#[cfg(feature = "daytona")]
impl DaytonaConfig {
    /// The validated account settings, or `None` when Daytona is not set up.
    /// `HERDR_DAYTONA_API_URL` replaces `api_url`.
    pub fn settings(&self) -> Result<Option<crate::daytona::Settings>> {
        self.settings_with(|name| env::var_os(name))
    }

    pub fn settings_with(
        &self,
        var: impl Fn(&str) -> Option<OsString>,
    ) -> Result<Option<crate::daytona::Settings>> {
        Ok(crate::daytona::Settings::resolve(self, var)?)
    }
}

/// The `[daytona]` keys the Settings window edits. An empty value removes its
/// key.
#[cfg(feature = "daytona")]
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DaytonaFields {
    pub api_url: String,
    pub organization_id: String,
    pub target: String,
    pub snapshot: String,
}

#[cfg(feature = "daytona")]
impl DaytonaFields {
    pub fn from_config(config: &DaytonaConfig) -> Self {
        let text = |value: &Option<String>| value.clone().unwrap_or_default();
        Self {
            api_url: text(&config.api_url),
            organization_id: text(&config.organization_id),
            target: text(&config.target),
            snapshot: text(&config.snapshot),
        }
    }

    pub fn entries(&self) -> [(&'static str, &str); 4] {
        [
            ("api_url", &self.api_url),
            ("organization_id", &self.organization_id),
            ("target", &self.target),
            ("snapshot", &self.snapshot),
        ]
    }

    /// The checks loading applies, so a save cannot leave a config that then
    /// fails to load.
    pub fn validate(&self, existing: &DaytonaConfig) -> Result<()> {
        let value = |text: &str| {
            let text = text.trim();
            (!text.is_empty()).then(|| text.to_owned())
        };
        let config = DaytonaConfig {
            api_url: value(&self.api_url),
            organization_id: value(&self.organization_id),
            target: value(&self.target),
            snapshot: value(&self.snapshot),
            ..existing.clone()
        };
        config.settings_with(|_| None).map(drop)
    }
}
