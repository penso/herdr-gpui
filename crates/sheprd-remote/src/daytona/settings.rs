//! The validated `[daytona]` table. Parsed once at the boundary so every later
//! request works with a known-good API root and identifiers.

use super::{Error, Result};
use crate::cloud::Value;
use crate::{config::DaytonaConfig, credentials::Store};
use std::ffi::OsString;
use url::Url;

/// The API root of Daytona's own cloud, offered when Settings is empty.
pub const DEFAULT_API_URL: &str = "https://app.daytona.io/api";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Settings {
    /// API root without a trailing slash, e.g. `https://app.daytona.io/api`.
    pub base: String,
    pub organization: Option<String>,
    pub target: Option<String>,
    pub snapshot: Option<String>,
    pub store: Store,
}

impl Settings {
    pub fn resolve(
        config: &DaytonaConfig,
        var: impl Fn(&str) -> Option<OsString>,
    ) -> Result<Option<Self>> {
        let url = match var("HERDR_DAYTONA_API_URL") {
            Some(value) => Some(
                value
                    .into_string()
                    .map_err(|_| Error::Encoding(Value::Key("HERDR_DAYTONA_API_URL")))?,
            ),
            None => config.api_url.clone(),
        };
        let Some(url) = url else {
            return Ok(None);
        };
        let identifier = |value: &Option<String>, field| match value {
            Some(text) if !identifier(text) => Err(Error::Field(Value::Key(field))),
            _ => Ok(value.clone()),
        };
        Ok(Some(Self {
            base: api_root(&url)?,
            organization: identifier(&config.organization_id, "daytona.organization_id")?,
            target: identifier(&config.target, "daytona.target")?,
            snapshot: identifier(&config.snapshot, "daytona.snapshot")?,
            store: Store::for_policy(config.allow_plaintext_credentials),
        }))
    }
}

/// Organization IDs, regions, and snapshot names travel in headers and JSON;
/// only plain identifiers are accepted. Snapshot names may carry an image tag.
fn identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 256
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.' | b':' | b'/'))
}

fn loopback(url: &Url) -> bool {
    match url.host() {
        Some(url::Host::Domain(host)) => host.eq_ignore_ascii_case("localhost"),
        Some(url::Host::Ipv4(ip)) => ip.is_loopback(),
        Some(url::Host::Ipv6(ip)) => ip.is_loopback(),
        None => false,
    }
}

fn api_root(text: &str) -> Result<String> {
    let url = Url::parse(text.trim()).map_err(|_| Error::Url("daytona.api_url"))?;
    let plain = url.username().is_empty()
        && url.password().is_none()
        && url.query().is_none()
        && url.fragment().is_none()
        && url.host().is_some();
    if !plain || !(url.scheme() == "https" || (url.scheme() == "http" && loopback(&url))) {
        return Err(Error::Url("daytona.api_url"));
    }
    Ok(url.as_str().trim_end_matches('/').to_owned())
}

#[cfg(test)]
mod tests;
