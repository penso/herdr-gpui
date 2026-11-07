//! The validated `[boxd]` table. Parsed once at the boundary so every later
//! request works with a known-good API origin and labels.

use super::{Error, Result};
use crate::cloud::Value;
use crate::{config::BoxdConfig, credentials::Store};
use std::{collections::BTreeMap, ffi::OsString};
use url::Url;

/// boxd's own API, offered when Settings is empty. A region other than the
/// account's home is `https://<region>.boxd.sh:9443`, such as `nyc`.
pub const DEFAULT_API_URL: &str = "https://boxd.sh:9443";
/// The label every machine made here carries, so the tab can count them.
pub const DEVICE_LABEL: (&str, &str) = ("sheprd", "device");

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Settings {
    /// The API origin without a trailing slash, e.g. `https://boxd.sh:9443`.
    pub base: String,
    /// Labels added to every new machine, beside [`DEVICE_LABEL`].
    pub labels: BTreeMap<String, String>,
    pub store: Store,
}

/// Where the exec stream connects.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Endpoint {
    pub host: String,
    pub port: u16,
    /// TLS, always but on loopback.
    pub encrypted: bool,
}

impl Settings {
    /// The settings, or `None` when boxd is not set up.
    /// `HERDR_BOXD_API_URL` replaces `api_url`.
    pub fn resolve(
        config: &BoxdConfig,
        var: impl Fn(&str) -> Option<OsString>,
    ) -> Result<Option<Self>> {
        let url = match var("HERDR_BOXD_API_URL") {
            Some(value) => Some(
                value
                    .into_string()
                    .map_err(|_| Error::Encoding(Value::Key("HERDR_BOXD_API_URL")))?,
            ),
            None => config.api_url.clone(),
        };
        let Some(url) = url else {
            return Ok(None);
        };
        for (key, value) in &config.labels {
            if !label_key(key) || key == DEVICE_LABEL.0 || !label_value(value) {
                return Err(Error::Field(Value::Key("boxd.labels")));
            }
        }
        Ok(Some(Self {
            base: api_root(&url)?,
            labels: config.labels.clone(),
            store: Store::for_policy(config.allow_plaintext_credentials),
        }))
    }

    /// The host and port the exec stream connects to.
    pub fn endpoint(&self) -> Result<Endpoint> {
        let url = Url::parse(&self.base).map_err(|_| Error::Url("boxd.api_url"))?;
        Ok(Endpoint {
            host: url
                .host_str()
                .ok_or(Error::Url("boxd.api_url"))?
                .trim_matches(['[', ']'])
                .to_owned(),
            port: url
                .port_or_known_default()
                .ok_or(Error::Url("boxd.api_url"))?,
            encrypted: url.scheme() == "https",
        })
    }

    /// The `Host` header for the exec stream, as the URL writes it.
    pub fn authority(&self) -> &str {
        self.base
            .split_once("://")
            .map_or(self.base.as_str(), |(_, authority)| authority)
    }
}

fn loopback(url: &Url) -> bool {
    match url.host() {
        Some(url::Host::Domain(host)) => host.eq_ignore_ascii_case("localhost"),
        Some(url::Host::Ipv4(ip)) => ip.is_loopback(),
        Some(url::Host::Ipv6(ip)) => ip.is_loopback(),
        None => false,
    }
}

/// The API is served at the root of its origin, so a path is refused rather
/// than silently dropped.
fn api_root(text: &str) -> Result<String> {
    let url = Url::parse(text.trim()).map_err(|_| Error::Url("boxd.api_url"))?;
    let plain = url.username().is_empty()
        && url.password().is_none()
        && url.query().is_none()
        && url.fragment().is_none()
        && url.path() == "/"
        && url.host().is_some();
    if !plain || !(url.scheme() == "https" || (url.scheme() == "http" && loopback(&url))) {
        return Err(Error::Url("boxd.api_url"));
    }
    Ok(url.as_str().trim_end_matches('/').to_owned())
}

/// boxd's rule for machine names: 1–63 lowercase letters, digits, and
/// hyphens, starting with a letter and not ending with a hyphen.
pub fn machine_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 63
        && value.starts_with(|c: char| c.is_ascii_lowercase())
        && !value.ends_with('-')
        && value
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

/// A machine as an API call names it: its ID or its name. Plain ASCII only.
pub fn machine_reference(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 63
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-')
}

/// boxd's rule for label keys: 1–63 of `[a-z0-9._/-]`, starting and ending
/// with a letter or digit; `boxd/` is reserved.
fn label_key(key: &str) -> bool {
    let edge = |b: u8| b.is_ascii_lowercase() || b.is_ascii_digit();
    !key.is_empty()
        && key.len() <= 63
        && !key.starts_with("boxd/")
        && key.bytes().next().is_some_and(edge)
        && key.bytes().last().is_some_and(edge)
        && key
            .bytes()
            .all(|b| edge(b) || matches!(b, b'.' | b'_' | b'/' | b'-'))
}

fn label_value(value: &str) -> bool {
    value.len() <= 256 && !value.chars().any(char::is_control)
}
