//! The validated `[sprites]` table. Parsed once at the boundary so every
//! later request works with a known-good API root.

use super::{Error, Result};
use crate::cloud::Value;
use crate::{config::SpritesConfig, credentials::Store};
use std::ffi::OsString;
use url::Url;

/// The API root of Fly's own Sprites service, offered when Settings is empty.
pub const DEFAULT_API_URL: &str = "https://api.sprites.dev";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Settings {
    /// API root without a trailing slash, e.g. `https://api.sprites.dev`.
    pub base: String,
    pub store: Store,
}

impl Settings {
    /// The settings, or `None` when Sprites is not set up.
    /// `HERDR_SPRITES_API_URL` replaces `api_url`.
    pub fn resolve(
        config: &SpritesConfig,
        var: impl Fn(&str) -> Option<OsString>,
    ) -> Result<Option<Self>> {
        let url = match var("HERDR_SPRITES_API_URL") {
            Some(value) => Some(
                value
                    .into_string()
                    .map_err(|_| Error::Encoding(Value::Key("HERDR_SPRITES_API_URL")))?,
            ),
            None => config.api_url.clone(),
        };
        let Some(url) = url else {
            return Ok(None);
        };
        Ok(Some(Self {
            base: api_root(&url)?,
            store: Store::for_policy(config.allow_plaintext_credentials),
        }))
    }

    /// The exec stream's URL for running `command` with `sh -c` on `sprite`,
    /// with stdin on: `wss` for an `https` root, `ws` for a loopback one.
    pub fn exec_url(&self, sprite: &str, command: &str) -> Result<Url> {
        let mut url = Url::parse(&self.base).map_err(|_| Error::Url("sprites.api_url"))?;
        let scheme = if url.scheme() == "https" { "wss" } else { "ws" };
        url.set_scheme(scheme)
            .map_err(|()| Error::Url("sprites.api_url"))?;
        url.path_segments_mut()
            .map_err(|()| Error::Url("sprites.api_url"))?
            .pop_if_empty()
            .extend(["v1", "sprites", sprite, "exec"]);
        url.query_pairs_mut()
            .append_pair("cmd", "/bin/sh")
            .append_pair("cmd", "-c")
            .append_pair("cmd", command)
            .append_pair("stdin", "true");
        Ok(url)
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

fn api_root(text: &str) -> Result<String> {
    let url = Url::parse(text.trim()).map_err(|_| Error::Url("sprites.api_url"))?;
    let plain = url.username().is_empty()
        && url.password().is_none()
        && url.query().is_none()
        && url.fragment().is_none()
        && url.host().is_some();
    if !plain || !(url.scheme() == "https" || (url.scheme() == "http" && loopback(&url))) {
        return Err(Error::Url("sprites.api_url"));
    }
    Ok(url.as_str().trim_end_matches('/').to_owned())
}

/// Sprite names travel in URL paths; only plain names are accepted.
pub fn sprite_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 63
        && !value.starts_with('-')
        && value
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}
