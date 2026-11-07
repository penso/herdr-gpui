//! The `[opensandbox]` table: a self-hosted OpenSandbox server whose
//! sandboxes become devices. A build without the `opensandbox` feature still
//! parses the table, so one config file serves every build, but never reads
//! it. The API key is never read from here: Settings keeps it in the
//! credential store.
#[cfg(feature = "opensandbox")]
use crate::Result;
use serde::Deserialize;
#[cfg(feature = "opensandbox")]
use std::{env, ffi::OsString};

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct OpenSandboxConfig {
    // keep-sorted start
    pub allow_plaintext_credentials: bool,
    /// The server's API root; https, or http on this computer.
    pub api_url: Option<String>,
    /// CPU for each new sandbox, as a quantity such as `2` or `500m`.
    pub cpu: Option<String>,
    /// The image new sandboxes start from.
    pub image: Option<String>,
    /// Memory for each new sandbox, such as `4Gi`.
    pub memory: Option<String>,
    // keep-sorted end
}

#[cfg(feature = "opensandbox")]
impl OpenSandboxConfig {
    /// The validated server settings, or `None` when OpenSandbox is not set
    /// up. `HERDR_OPENSANDBOX_API_URL` replaces `api_url`.
    pub fn settings(&self) -> Result<Option<crate::opensandbox::Settings>> {
        self.settings_with(|name| env::var_os(name))
    }

    pub fn settings_with(
        &self,
        var: impl Fn(&str) -> Option<OsString>,
    ) -> Result<Option<crate::opensandbox::Settings>> {
        Ok(crate::opensandbox::Settings::resolve(self, var)?)
    }
}

/// The `[opensandbox]` keys the Settings window edits. An empty value removes
/// its key.
#[cfg(feature = "opensandbox")]
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct OpenSandboxFields {
    pub api_url: String,
    pub image: String,
    pub cpu: String,
    pub memory: String,
}

#[cfg(feature = "opensandbox")]
impl OpenSandboxFields {
    pub fn from_config(config: &OpenSandboxConfig) -> Self {
        let text = |value: &Option<String>| value.clone().unwrap_or_default();
        Self {
            api_url: text(&config.api_url),
            image: text(&config.image),
            cpu: text(&config.cpu),
            memory: text(&config.memory),
        }
    }

    pub fn entries(&self) -> [(&'static str, &str); 4] {
        [
            ("api_url", &self.api_url),
            ("image", &self.image),
            ("cpu", &self.cpu),
            ("memory", &self.memory),
        ]
    }

    /// The checks loading applies, so a save cannot leave a config that then
    /// fails to load.
    pub fn validate(&self, existing: &OpenSandboxConfig) -> Result<()> {
        let value = |text: &str| {
            let text = text.trim();
            (!text.is_empty()).then(|| text.to_owned())
        };
        OpenSandboxConfig {
            api_url: value(&self.api_url),
            image: value(&self.image),
            cpu: value(&self.cpu),
            memory: value(&self.memory),
            ..existing.clone()
        }
        .settings_with(|_| None)
        .map(drop)
    }
}
