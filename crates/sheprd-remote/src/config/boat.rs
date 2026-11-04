//! The `[boat]` table: a Boat account whose sandboxes become devices. A build
//! without the `boat` feature still parses the table, so one config file serves
//! every build, but never reads it. The API key is kept in the credential store.
#[cfg(feature = "boat")]
use crate::Result;
use serde::Deserialize;
#[cfg(feature = "boat")]
use std::{env, ffi::OsString};

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct BoatConfig {
    /// The API root, `https://boat.dev/api/v1` for Boat's cloud.
    pub api_url: Option<String>,
    /// An organization ID or name; the active wallet if unset or empty.
    pub organization: Option<String>,
    /// `small`, `default`, `large`, or `xlarge`; Boat's default if unset or empty.
    pub machine_type: Option<String>,
    /// A named Boat environment; the account's default if unset or empty.
    pub environment: Option<String>,
    /// `never` (default), or a number of seconds from create, resume or extension.
    pub auto_stop: Option<String>,
    pub allow_plaintext_credentials: bool,
}

#[cfg(feature = "boat")]
impl BoatConfig {
    /// The validated account settings, or `None` when Boat is not set up.
    /// `HERDR_BOAT_API_URL` replaces `api_url`.
    pub fn settings(&self) -> Result<Option<crate::boat::Settings>> {
        self.settings_with(|name| env::var_os(name))
    }

    pub fn settings_with(
        &self,
        var: impl Fn(&str) -> Option<OsString>,
    ) -> Result<Option<crate::boat::Settings>> {
        Ok(crate::boat::Settings::resolve(self, var)?)
    }
}

/// The `[boat]` keys the Settings window edits. An empty value removes its key.
#[cfg(feature = "boat")]
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BoatFields {
    pub api_url: String,
    pub organization: String,
    pub machine_type: String,
    pub environment: String,
    pub auto_stop: String,
}

#[cfg(feature = "boat")]
impl BoatFields {
    pub fn from_config(config: &BoatConfig) -> Self {
        let text = |value: &Option<String>| value.clone().unwrap_or_default();
        Self {
            api_url: text(&config.api_url),
            organization: text(&config.organization),
            machine_type: text(&config.machine_type),
            environment: text(&config.environment),
            auto_stop: config.auto_stop.clone().unwrap_or_else(|| "never".into()),
        }
    }

    pub fn entries(&self) -> [(&'static str, &str); 5] {
        [
            ("api_url", &self.api_url),
            ("organization", &self.organization),
            ("machine_type", &self.machine_type),
            ("environment", &self.environment),
            ("auto_stop", &self.auto_stop),
        ]
    }

    pub fn validate(&self, existing: &BoatConfig) -> Result<()> {
        let value = |text: &str| {
            let text = text.trim();
            (!text.is_empty()).then(|| text.to_owned())
        };
        let config = BoatConfig {
            api_url: value(&self.api_url),
            organization: value(&self.organization),
            machine_type: value(&self.machine_type),
            environment: value(&self.environment),
            auto_stop: value(&self.auto_stop),
            ..existing.clone()
        };
        config.settings_with(|_| None).map(drop)
    }
}
