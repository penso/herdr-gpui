//! The `[boxd]` table: a boxd account whose machines become devices. A build
//! without the `boxd` feature still parses the table, so one config file
//! serves every build, but never reads it. The API key is never read from
//! here: Settings keeps it in the credential store.
#[cfg(feature = "boxd")]
use crate::Result;
use serde::Deserialize;
use std::collections::BTreeMap;
#[cfg(feature = "boxd")]
use std::{env, ffi::OsString};

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct BoxdConfig {
    // keep-sorted start
    pub allow_plaintext_credentials: bool,
    /// The API origin, `https://boxd.sh:9443` for boxd's own service.
    pub api_url: Option<String>,
    /// Labels added to every machine made here, such as `team = "web"`.
    pub labels: BTreeMap<String, String>,
    // keep-sorted end
}

#[cfg(feature = "boxd")]
impl BoxdConfig {
    /// The validated account settings, or `None` when boxd is not set up.
    /// `HERDR_BOXD_API_URL` replaces `api_url`.
    pub fn settings(&self) -> Result<Option<crate::boxd::Settings>> {
        self.settings_with(|name| env::var_os(name))
    }

    pub fn settings_with(
        &self,
        var: impl Fn(&str) -> Option<OsString>,
    ) -> Result<Option<crate::boxd::Settings>> {
        Ok(crate::boxd::Settings::resolve(self, var)?)
    }
}

/// The `[boxd]` keys the Settings window edits. An empty value removes its
/// key.
#[cfg(feature = "boxd")]
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BoxdFields {
    pub api_url: String,
}

#[cfg(feature = "boxd")]
impl BoxdFields {
    pub fn from_config(config: &BoxdConfig) -> Self {
        Self {
            api_url: config.api_url.clone().unwrap_or_default(),
        }
    }

    pub fn entries(&self) -> [(&'static str, &str); 1] {
        [("api_url", &self.api_url)]
    }

    /// The checks loading applies, so a save cannot leave a config that then
    /// fails to load.
    pub fn validate(&self, existing: &BoxdConfig) -> Result<()> {
        let api_url = self.api_url.trim();
        BoxdConfig {
            api_url: (!api_url.is_empty()).then(|| api_url.to_owned()),
            ..existing.clone()
        }
        .settings_with(|_| None)
        .map(drop)
    }
}
