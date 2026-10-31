//! The validated `[opensandbox]` table. Parsed once at the boundary so every
//! later request works with a known-good server root, image, and limits.

use super::{Error, Result};
use crate::cloud::Value;
use crate::{config::OpenSandboxConfig, credentials::Store};
use std::ffi::OsString;
use url::Url;

/// A server run on this computer with `opensandbox-server`'s defaults.
pub const DEFAULT_API_URL: &str = "http://127.0.0.1:8080";
pub const DEFAULT_IMAGE: &str = "buildpack-deps:bookworm";
pub const DEFAULT_CPU: &str = "2";
pub const DEFAULT_MEMORY: &str = "4Gi";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Settings {
    /// API root without a trailing slash.
    pub base: String,
    pub image: String,
    pub cpu: String,
    pub memory: String,
    pub store: Store,
}

impl Settings {
    /// The settings, or `None` when OpenSandbox is not set up.
    /// `HERDR_OPENSANDBOX_API_URL` replaces `api_url`.
    pub fn resolve(
        config: &OpenSandboxConfig,
        var: impl Fn(&str) -> Option<OsString>,
    ) -> Result<Option<Self>> {
        let url = match var("HERDR_OPENSANDBOX_API_URL") {
            Some(value) => Some(
                value
                    .into_string()
                    .map_err(|_| Error::Encoding(Value::Key("HERDR_OPENSANDBOX_API_URL")))?,
            ),
            None => config.api_url.clone(),
        };
        let Some(url) = url else {
            return Ok(None);
        };
        let text = |value: &Option<String>, default: &str| {
            value
                .as_deref()
                .map(str::trim)
                .filter(|text| !text.is_empty())
                .unwrap_or(default)
                .to_owned()
        };
        let image = text(&config.image, DEFAULT_IMAGE);
        if !crate::cloud::names::image_reference(&image) {
            return Err(Error::Field(Value::Key("opensandbox.image")));
        }
        let cpu = text(&config.cpu, DEFAULT_CPU);
        if !quantity(&cpu) {
            return Err(Error::Field(Value::Key("opensandbox.cpu")));
        }
        let memory = text(&config.memory, DEFAULT_MEMORY);
        if !quantity(&memory) {
            return Err(Error::Field(Value::Key("opensandbox.memory")));
        }
        Ok(Some(Self {
            base: api_root(&url)?,
            image,
            cpu,
            memory,
            store: Store::for_policy(config.allow_plaintext_credentials),
        }))
    }

    /// The scheme an endpoint the server names is reached with: the API
    /// root's own, so a TLS server's sandboxes are reached over TLS too.
    pub fn endpoint_scheme(&self) -> &'static str {
        if self.base.starts_with("https://") {
            "https"
        } else {
            "http"
        }
    }
}

/// A Kubernetes-style quantity such as `2`, `500m`, or `4Gi`.
pub fn quantity(value: &str) -> bool {
    let digits = value.trim_end_matches(|c: char| c.is_ascii_alphabetic());
    let unit = &value[digits.len()..];
    !digits.is_empty()
        && digits.len() <= 12
        && digits.bytes().all(|b| b.is_ascii_digit() || b == b'.')
        && digits.parse::<f64>().is_ok_and(|number| number > 0.)
        && ["", "m", "k", "M", "G", "T", "Ki", "Mi", "Gi", "Ti"].contains(&unit)
}

pub(super) fn loopback(url: &Url) -> bool {
    match url.host() {
        Some(url::Host::Domain(host)) => host.eq_ignore_ascii_case("localhost"),
        Some(url::Host::Ipv4(ip)) => ip.is_loopback(),
        Some(url::Host::Ipv6(ip)) => ip.is_loopback(),
        None => false,
    }
}

fn api_root(text: &str) -> Result<String> {
    let url = Url::parse(text.trim()).map_err(|_| Error::Url("opensandbox.api_url"))?;
    let plain = url.username().is_empty()
        && url.password().is_none()
        && url.query().is_none()
        && url.fragment().is_none()
        && url.host().is_some();
    if !plain || !(url.scheme() == "https" || (url.scheme() == "http" && loopback(&url))) {
        return Err(Error::Url("opensandbox.api_url"));
    }
    Ok(url.as_str().trim_end_matches('/').to_owned())
}

/// Sandbox IDs travel in URL paths; only plain IDs are accepted.
pub fn sandbox_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && !value.starts_with('-')
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'))
}
