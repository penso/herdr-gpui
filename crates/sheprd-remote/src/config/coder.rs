//! The `[coder]` table: a Coder deployment whose workspaces become devices.
//! A build without the `coder` feature still parses the table, so one config
//! file serves every build, but never reads it.
#[cfg(feature = "coder")]
use crate::Result;
use serde::Deserialize;
use std::path::PathBuf;
#[cfg(feature = "coder")]
use std::{env, ffi::OsString};

/// A self-hosted Coder deployment whose workspaces can be added as devices.
/// Coder's OAuth2 provider requires a confidential client. Its secret may be
/// set here or in `HERDR_CODER_OAUTH_CLIENT_SECRET`, but Settings saves it to
/// the credential store instead; it is redacted from debug output.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct CoderConfig {
    pub url: Option<String>,
    pub oauth_client_id: Option<String>,
    pub oauth_client_secret: Option<secrecy::SecretString>,
    pub oauth_redirect_uri: Option<String>,
    pub organization: Option<String>,
    pub workspace_prefix: Option<String>,
    /// Absolute path to the `coder` CLI when it is not on PATH.
    pub cli: Option<PathBuf>,
    pub allow_plaintext_credentials: bool,
}

#[cfg(feature = "coder")]
impl CoderConfig {
    /// The validated deployment settings, or `None` when Coder is not set up.
    /// Each `HERDR_CODER_*` variable replaces the matching key.
    pub fn settings(&self) -> Result<Option<crate::coder::Settings>> {
        self.settings_with(|name| env::var_os(name))
    }

    pub fn settings_with(
        &self,
        var: impl Fn(&str) -> Option<OsString>,
    ) -> Result<Option<crate::coder::Settings>> {
        Ok(crate::coder::Settings::resolve(self, var)?)
    }

    /// The values this table sets, each checked alone; see `Settings::check`.
    pub fn check(&self) -> Result<()> {
        Ok(crate::coder::Settings::check(self)?)
    }
}

/// The `[coder]` keys the Settings window edits. An empty value removes its
/// key; the client secret and the plaintext opt-in are never written here.
#[cfg(feature = "coder")]
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CoderFields {
    pub url: String,
    pub oauth_client_id: String,
    pub oauth_redirect_uri: String,
    pub organization: String,
    pub workspace_prefix: String,
    pub cli: String,
}

#[cfg(feature = "coder")]
impl CoderFields {
    pub fn from_config(config: &CoderConfig) -> Self {
        let text = |value: &Option<String>| value.clone().unwrap_or_default();
        Self {
            url: text(&config.url),
            oauth_client_id: text(&config.oauth_client_id),
            oauth_redirect_uri: text(&config.oauth_redirect_uri),
            organization: text(&config.organization),
            workspace_prefix: text(&config.workspace_prefix),
            cli: config
                .cli
                .as_ref()
                .map(|path| path.display().to_string())
                .unwrap_or_default(),
        }
    }

    pub fn entries(&self) -> [(&'static str, &str); 6] {
        [
            ("url", &self.url),
            ("oauth_client_id", &self.oauth_client_id),
            ("oauth_redirect_uri", &self.oauth_redirect_uri),
            ("organization", &self.organization),
            ("workspace_prefix", &self.workspace_prefix),
            ("cli", &self.cli),
        ]
    }

    /// The same checks loading applies, so a save cannot leave a config that
    /// then fails to load. A missing secret is allowed: Settings stores it.
    pub fn validate(&self, existing: &CoderConfig) -> Result<()> {
        let value = |text: &str| {
            let text = text.trim();
            (!text.is_empty()).then(|| text.to_owned())
        };
        let config = CoderConfig {
            url: value(&self.url),
            oauth_client_id: value(&self.oauth_client_id),
            oauth_redirect_uri: value(&self.oauth_redirect_uri),
            organization: value(&self.organization),
            workspace_prefix: value(&self.workspace_prefix),
            cli: value(&self.cli).map(PathBuf::from),
            ..existing.clone()
        };
        config.check()
    }
}
