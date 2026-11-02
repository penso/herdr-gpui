//! The validated `[boat]` table. Parsed once at the boundary so every later
//! request works with a known-good API root and create options.

use super::{Error, Result};
use crate::cloud::Value;
use crate::{config::BoatConfig, credentials::Store};
use std::ffi::OsString;
use url::Url;

/// The API root of Boat's cloud, offered when Settings is empty.
pub const DEFAULT_API_URL: &str = "https://boat.dev/api/v1";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Settings {
    pub base: String,
    pub organization: Option<String>,
    pub machine_type: Option<String>,
    pub environment: Option<String>,
    pub ttl_seconds: Option<u32>,
    pub store: Store,
}

impl Settings {
    pub fn resolve(
        config: &BoatConfig,
        var: impl Fn(&str) -> Option<OsString>,
    ) -> Result<Option<Self>> {
        let url = match var("HERDR_BOAT_API_URL") {
            Some(value) => Some(
                value
                    .into_string()
                    .map_err(|_| Error::Encoding(Value::Key("HERDR_BOAT_API_URL")))?,
            ),
            None => config.api_url.clone(),
        };
        let Some(url) = url else {
            return Ok(None);
        };
        let plain = |value: &Option<String>, field| match value {
            Some(text) if text.is_empty() => Ok(None),
            Some(text) if !plain_value(text) => Err(Error::Field(Value::Key(field))),
            _ => Ok(value.clone()),
        };
        let machine_type = config
            .machine_type
            .as_deref()
            .filter(|kind| !kind.is_empty());
        if machine_type
            .is_some_and(|kind| !matches!(kind, "small" | "default" | "large" | "xlarge"))
        {
            return Err(Error::Field(Value::Key("boat.machine_type")));
        }
        Ok(Some(Self {
            base: api_root(&url)?,
            organization: plain(&config.organization, "boat.organization")?,
            machine_type: machine_type.map(str::to_owned),
            environment: plain(&config.environment, "boat.environment")?,
            ttl_seconds: lifetime(config.auto_stop.as_deref().unwrap_or("never"))?,
            store: Store::for_policy(config.allow_plaintext_credentials),
        }))
    }

    /// Stable account identity saved with devices. Organization changes must
    /// not silently move a device to a different billing wallet.
    pub fn account(&self) -> String {
        match &self.organization {
            Some(organization) => format!("{}#{organization}", self.base),
            None => self.base.clone(),
        }
    }
}

fn lifetime(text: &str) -> Result<Option<u32>> {
    if text == "never" {
        return Ok(None);
    }
    text.parse::<u32>()
        .ok()
        .filter(|seconds| (1..=2_592_000).contains(seconds))
        .map(Some)
        .ok_or(Error::Field(Value::BoatAutoStop))
}

fn plain_value(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 256
        && value.trim() == value
        && !value.contains('#')
        && value
            .bytes()
            .all(|byte| byte == b' ' || byte.is_ascii_graphic())
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
    let url = Url::parse(text.trim()).map_err(|_| Error::Url("boat.api_url"))?;
    let plain = url.username().is_empty()
        && url.password().is_none()
        && url.query().is_none()
        && url.fragment().is_none()
        && url.host().is_some();
    if !plain || !(url.scheme() == "https" || (url.scheme() == "http" && loopback(&url))) {
        return Err(Error::Url("boat.api_url"));
    }
    Ok(url.as_str().trim_end_matches('/').to_owned())
}

#[cfg(test)]
mod tests;
