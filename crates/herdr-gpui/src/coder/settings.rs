//! The validated `[coder]` table. Parsed once at the boundary so every later
//! request works with a known-good deployment URL, client, and redirect.

use super::{Error, Result};
use crate::{config::CoderConfig, github::Store};
use secrecy::{ExposeSecret, SecretString};
use std::{
    ffi::OsString,
    net::{IpAddr, Ipv4Addr, SocketAddr},
    path::PathBuf,
};
use url::Url;

const DEFAULT_PREFIX: &str = "herdr";
/// Coder caps workspace names at 32 characters; the prefix leaves room for a suffix.
pub(super) const PREFIX_LIMIT: usize = 16;

#[derive(Clone, Debug)]
pub(crate) struct Settings {
    /// Deployment root without a trailing slash, e.g. `https://coder.example.com`.
    pub(crate) base: String,
    pub(crate) client_id: String,
    pub(crate) client_secret: SecretString,
    pub(crate) redirect: Redirect,
    pub(crate) organization: Option<String>,
    pub(crate) workspace_prefix: String,
    /// An explicit `coder` executable; otherwise it is found on PATH.
    pub(crate) cli: Option<PathBuf>,
    pub(crate) store: Store,
}

/// The loopback redirect registered with the Coder OAuth2 app. Coder compares
/// redirect URIs as exact strings, so the configured text is sent unchanged.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Redirect {
    pub(crate) uri: String,
    pub(crate) path: String,
    pub(crate) address: SocketAddr,
}

impl Settings {
    pub(crate) fn resolve(
        config: &CoderConfig,
        var: impl Fn(&str) -> Option<OsString>,
    ) -> Result<Option<Self>> {
        let text = |name: &'static str, value: &Option<String>| -> Result<Option<String>> {
            match var(name) {
                Some(value) => value
                    .into_string()
                    .map(Some)
                    .map_err(|_| Error::Encoding(name)),
                None => Ok(value.clone()),
            }
        };
        let Some(url) = text("HERDR_CODER_URL", &config.url)? else {
            return Ok(None);
        };
        let base = deployment(&url)?;
        let client_id = text("HERDR_CODER_OAUTH_CLIENT_ID", &config.oauth_client_id)?
            .ok_or(Error::Missing("oauth_client_id"))?;
        if !identifier(&client_id, 256) {
            return Err(Error::Field("coder.oauth_client_id"));
        }
        let client_secret = match var("HERDR_CODER_OAUTH_CLIENT_SECRET") {
            Some(value) => {
                // Own and wipe the copy, even when it is not valid UTF-8.
                let bytes = zeroize::Zeroizing::new(value.into_encoded_bytes());
                std::str::from_utf8(&bytes)
                    .map(SecretString::from)
                    .map_err(|_| Error::Encoding("HERDR_CODER_OAUTH_CLIENT_SECRET"))?
            }
            None => config
                .oauth_client_secret
                .clone()
                .ok_or(Error::Missing("oauth_client_secret"))?,
        };
        if !super::token::valid(client_secret.expose_secret()) {
            return Err(Error::Field("coder.oauth_client_secret"));
        }
        let redirect = redirect(
            &text("HERDR_CODER_OAUTH_REDIRECT_URI", &config.oauth_redirect_uri)?
                .ok_or(Error::Missing("oauth_redirect_uri"))?,
        )?;
        let organization = config.organization.clone();
        if organization
            .as_deref()
            .is_some_and(|name| !identifier(name, 64))
        {
            return Err(Error::Field("coder.organization"));
        }
        let workspace_prefix = config
            .workspace_prefix
            .clone()
            .unwrap_or_else(|| DEFAULT_PREFIX.into());
        if !super::names::valid(&workspace_prefix) || workspace_prefix.len() > PREFIX_LIMIT {
            return Err(Error::Field("coder.workspace_prefix"));
        }
        let cli = config.cli.clone();
        if cli.as_deref().is_some_and(|path| !path.is_absolute()) {
            return Err(Error::Field("coder.cli"));
        }
        Ok(Some(Self {
            cli,
            base,
            client_id,
            client_secret,
            redirect,
            organization,
            workspace_prefix,
            store: Store::for_policy(config.allow_plaintext_credentials),
        }))
    }

    /// `path` must start with `/`; it is appended to the deployment root.
    pub(crate) fn endpoint(&self, path: &str) -> String {
        format!("{}{path}", self.base)
    }
}

fn identifier(value: &str, limit: usize) -> bool {
    !value.is_empty()
        && value.len() <= limit
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.'))
}

fn loopback(url: &Url) -> bool {
    match url.host() {
        Some(url::Host::Domain(host)) => host.eq_ignore_ascii_case("localhost"),
        Some(url::Host::Ipv4(ip)) => ip.is_loopback(),
        Some(url::Host::Ipv6(ip)) => ip.is_loopback(),
        None => false,
    }
}

fn plain(url: &Url) -> bool {
    url.username().is_empty()
        && url.password().is_none()
        && url.query().is_none()
        && url.fragment().is_none()
        && url.host().is_some()
}

fn deployment(text: &str) -> Result<String> {
    let url = Url::parse(text.trim()).map_err(|_| Error::Url("coder.url"))?;
    if !plain(&url) || !(url.scheme() == "https" || (url.scheme() == "http" && loopback(&url))) {
        return Err(Error::Url("coder.url"));
    }
    Ok(url.as_str().trim_end_matches('/').to_owned())
}

fn redirect(text: &str) -> Result<Redirect> {
    let field = "coder.oauth_redirect_uri";
    let url = Url::parse(text).map_err(|_| Error::Url(field))?;
    // The listener binds this exact address, so a missing port is ambiguous.
    let port = url
        .port()
        .filter(|port| *port != 0)
        .ok_or(Error::Field(field))?;
    if url.scheme() != "http" || !plain(&url) || !loopback(&url) {
        return Err(Error::Url(field));
    }
    let ip = match url.host() {
        Some(url::Host::Ipv4(ip)) => IpAddr::V4(ip),
        Some(url::Host::Ipv6(ip)) => IpAddr::V6(ip),
        _ => IpAddr::V4(Ipv4Addr::LOCALHOST),
    };
    Ok(Redirect {
        uri: text.to_owned(),
        path: url.path().to_owned(),
        address: SocketAddr::new(ip, port),
    })
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    fn config() -> CoderConfig {
        CoderConfig {
            url: Some("https://coder.example.com/".into()),
            oauth_client_id: Some("0b0c1b3e-8f1a-4d6e-9d4a-8f0f6f0c2a11".into()),
            oauth_client_secret: Some("fixture-secret".into()),
            oauth_redirect_uri: Some("http://127.0.0.1:47823/callback".into()),
            ..CoderConfig::default()
        }
    }

    fn resolve(config: &CoderConfig) -> Result<Option<Settings>> {
        Settings::resolve(config, |_| None)
    }

    #[test]
    fn unconfigured_deployment_is_absent_not_an_error() {
        assert!(resolve(&CoderConfig::default()).unwrap().is_none());
    }

    #[test]
    fn valid_table_is_normalized_and_redirect_text_is_kept_verbatim() {
        let settings = resolve(&config()).unwrap().unwrap();
        assert_eq!(settings.base, "https://coder.example.com");
        assert_eq!(
            settings.endpoint("/oauth2/tokens"),
            "https://coder.example.com/oauth2/tokens"
        );
        assert_eq!(settings.redirect.uri, "http://127.0.0.1:47823/callback");
        assert_eq!(settings.redirect.path, "/callback");
        assert_eq!(
            settings.redirect.address,
            "127.0.0.1:47823".parse().unwrap()
        );
        assert_eq!(settings.workspace_prefix, "herdr");
        let debug = format!("{settings:?}");
        assert!(!debug.contains("fixture-secret"));

        let mut localhost = config();
        localhost.oauth_redirect_uri = Some("http://localhost:9000".into());
        let settings = resolve(&localhost).unwrap().unwrap();
        // Url would normalize this to `.../`; Coder must see what was registered.
        assert_eq!(settings.redirect.uri, "http://localhost:9000");
        assert_eq!(settings.redirect.address, "127.0.0.1:9000".parse().unwrap());
    }

    #[test]
    fn environment_replaces_each_key() {
        let settings = Settings::resolve(&config(), |name| match name {
            "HERDR_CODER_URL" => Some("https://other.example.com/coder".into()),
            "HERDR_CODER_OAUTH_CLIENT_SECRET" => Some("env-secret".into()),
            _ => None,
        })
        .unwrap()
        .unwrap();
        assert_eq!(settings.base, "https://other.example.com/coder");
        assert_eq!(settings.client_secret.expose_secret(), "env-secret");
        let only_env = Settings::resolve(&CoderConfig::default(), |name| match name {
            "HERDR_CODER_URL" => Some("https://coder.example.com".into()),
            _ => None,
        });
        assert!(matches!(only_env, Err(Error::Missing("oauth_client_id"))));
    }

    #[test]
    fn unsafe_or_ambiguous_values_are_rejected() {
        for (url, redirect) in [
            ("http://coder.example.com", "http://127.0.0.1:1/cb"),
            ("https://user:pw@coder.example.com", "http://127.0.0.1:1/cb"),
            ("https://coder.example.com?x=1", "http://127.0.0.1:1/cb"),
            ("https://coder.example.com#x", "http://127.0.0.1:1/cb"),
            ("ftp://coder.example.com", "http://127.0.0.1:1/cb"),
            ("not a url", "http://127.0.0.1:1/cb"),
            ("https://coder.example.com", "https://127.0.0.1:1/cb"),
            ("https://coder.example.com", "http://example.com:1/cb"),
            ("https://coder.example.com", "http://127.0.0.1/cb"),
            ("https://coder.example.com", "http://127.0.0.1:0/cb"),
            ("https://coder.example.com", "http://127.0.0.1:1/cb?x=1"),
        ] {
            let mut config = config();
            config.url = Some(url.into());
            config.oauth_redirect_uri = Some(redirect.into());
            assert!(resolve(&config).is_err(), "{url} {redirect}");
        }
        let mut local = config();
        local.url = Some("http://127.0.0.1:3000".into());
        assert!(resolve(&local).is_ok());
        for mutate in [
            (|c: &mut CoderConfig| c.oauth_client_id = Some("bad/id".into())) as fn(&mut _),
            |c| c.oauth_client_id = None,
            |c| c.oauth_client_secret = Some("bad secret".into()),
            |c| c.oauth_client_secret = None,
            |c| c.oauth_redirect_uri = None,
            |c| c.organization = Some("bad org".into()),
            |c| c.workspace_prefix = Some("Bad_Prefix".into()),
            |c| c.workspace_prefix = Some("a".repeat(PREFIX_LIMIT + 1)),
            |c| c.cli = Some("relative/coder".into()),
        ] {
            let mut config = config();
            mutate(&mut config);
            let error = resolve(&config).unwrap_err();
            assert!(!error.to_string().contains("fixture-secret"));
        }
    }
}
