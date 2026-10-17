//! Bounded HTTP client for Azure Resource Manager and Microsoft Entra ID.

use super::{Error, Result, Settings};
use secrecy::{ExposeSecret, ExposeSecretMut, SecretBox, SecretString};
use serde::{Serialize, de::DeserializeOwned};
use std::{io::Read, time::Duration};
use zeroize::Zeroizing;

const LIMIT: u64 = 8 * 1024 * 1024;
const TIMEOUT: Duration = Duration::from_secs(60);

/// Which Azure API a token is for. Resource Manager and the Container Apps
/// sandbox data plane ("ADC") accept only tokens issued for themselves.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Audience {
    ResourceManager,
    Sandboxes,
}

impl Audience {
    /// The resource the Azure CLI issues a token for.
    pub(crate) fn resource(self) -> &'static str {
        match self {
            Self::ResourceManager => "https://management.azure.com",
            Self::Sandboxes => "https://dynamicsessions.io",
        }
    }

    /// The OAuth scope, already form-encoded, of a client credentials grant.
    fn scope(self) -> &'static str {
        match self {
            Self::ResourceManager => "https%3A%2F%2Fmanagement.azure.com%2F.default",
            Self::Sandboxes => "https%3A%2F%2Fdynamicsessions.io%2F.default",
        }
    }
}

pub(crate) struct Client<'a> {
    pub(crate) settings: &'a Settings,
    token: SecretString,
}

impl<'a> Client<'a> {
    pub(crate) fn new(settings: &'a Settings) -> Result<Self> {
        Self::for_audience(settings, Audience::ResourceManager)
    }

    pub(crate) fn for_audience(settings: &'a Settings, audience: Audience) -> Result<Self> {
        let token = resolve_token(settings, audience)?;
        Ok(Self { settings, token })
    }

    /// The bearer token, for a request this client does not make itself.
    pub(crate) fn token(&self) -> &SecretString {
        &self.token
    }

    fn url(&self, path: &str) -> String {
        if path.starts_with("https://") {
            path.to_owned()
        } else {
            format!("https://management.azure.com{path}")
        }
    }

    pub(crate) fn get<T: DeserializeOwned + 'static>(&self, path: &str) -> Result<T> {
        let auth = authorization(&self.token)?;
        let mut response = agent()
            .get(&self.url(path))
            .header("Authorization", auth)
            .header("Content-Type", "application/json")
            .call()
            .map_err(network("GET"))?;
        handle_response(&mut response)
    }

    pub(crate) fn put<B: Serialize, T: DeserializeOwned + 'static>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<T> {
        let auth = authorization(&self.token)?;
        let json = serde_json::to_vec(body).map_err(Error::Json)?;
        let mut response = agent()
            .put(&self.url(path))
            .header("Authorization", auth)
            .header("Content-Type", "application/json")
            .send(&json)
            .map_err(network("PUT"))?;
        handle_response(&mut response)
    }

    #[allow(dead_code)]
    pub(crate) fn post<B: Serialize, T: DeserializeOwned + 'static>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<T> {
        let auth = authorization(&self.token)?;
        let json = serde_json::to_vec(body).map_err(Error::Json)?;
        let mut response = agent()
            .post(&self.url(path))
            .header("Authorization", auth)
            .header("Content-Type", "application/json")
            .send(&json)
            .map_err(network("POST"))?;
        handle_response(&mut response)
    }

    pub(crate) fn post_empty<T: DeserializeOwned + 'static>(&self, path: &str) -> Result<T> {
        let auth = authorization(&self.token)?;
        let mut response = agent()
            .post(&self.url(path))
            .header("Authorization", auth)
            .header("Content-Type", "application/json")
            .send_empty()
            .map_err(network("POST"))?;
        handle_response(&mut response)
    }

    pub(crate) fn delete<T: DeserializeOwned + 'static>(&self, path: &str) -> Result<T> {
        let auth = authorization(&self.token)?;
        let mut response = agent()
            .delete(&self.url(path))
            .header("Authorization", auth)
            .header("Content-Type", "application/json")
            .call()
            .map_err(network("DELETE"))?;
        handle_response(&mut response)
    }
}

/// An anonymous GET of a public Azure API, such as retail prices.
pub(crate) fn get_public<T: DeserializeOwned + 'static>(
    url: &str,
    query: &[(&str, &str)],
) -> Result<T> {
    let mut request = agent().get(url);
    for (name, value) in query {
        request = request.query(*name, *value);
    }
    let mut response = request.call().map_err(network("GET"))?;
    handle_response(&mut response)
}

fn handle_response<T: DeserializeOwned + 'static>(
    response: &mut ureq::http::Response<ureq::Body>,
) -> Result<T> {
    let status = response.status().as_u16();
    let bytes = read_body(response)?;

    if status >= 400 {
        let message = diagnostic(&bytes);
        return Err(match status {
            401 => Error::Authentication(message),
            403 => Error::Forbidden(message),
            _ if codes(&bytes).iter().any(|code| code == "SkuNotAvailable") => {
                Error::SkuNotAvailable(message)
            }
            _ => Error::Status(status, message),
        });
    }

    if std::any::TypeId::of::<T>() == std::any::TypeId::of::<()>() {
        return serde_json::from_str("null").map_err(Error::Json);
    }

    if bytes.is_empty() {
        return serde_json::from_str("{}").map_err(Error::Json);
    }

    serde_json::from_slice(&bytes).map_err(Error::Json)
}

fn resolve_token(settings: &Settings, audience: Audience) -> Result<SecretString> {
    if let (Some(tenant_id), Some(client_id)) = (&settings.tenant_id, &settings.client_id)
        && let Ok(Some(secret)) = super::store::secret(settings)
    {
        return oauth_token(tenant_id, client_id, &secret, audience);
    }

    if settings.use_azure_cli {
        let token = super::cli::access_token(audience, &settings.subscription_id)?;
        return Ok(token.into());
    }

    Err(Error::MissingCredentials)
}

#[derive(serde::Deserialize)]
struct TokenResponse {
    access_token: String,
}

fn oauth_token(
    tenant_id: &str,
    client_id: &str,
    secret: &SecretString,
    audience: Audience,
) -> Result<SecretString> {
    let url = format!(
        "https://login.microsoftonline.com/{}/oauth2/v2.0/token",
        tenant_id
    );

    let body = format!(
        "grant_type=client_credentials&client_id={}&client_secret={}&scope={}",
        percent_encoding::utf8_percent_encode(client_id, percent_encoding::NON_ALPHANUMERIC),
        percent_encoding::utf8_percent_encode(
            secret.expose_secret(),
            percent_encoding::NON_ALPHANUMERIC
        ),
        audience.scope(),
    );

    let mut response = agent()
        .post(&url)
        .header("Content-Type", "application/x-www-form-urlencoded")
        .send(body.as_bytes())
        .map_err(network("oauth_token"))?;

    let status = response.status().as_u16();
    let bytes = read_body(&mut response)?;

    if status != 200 {
        return Err(Error::Authentication(diagnostic(&bytes)));
    }

    let token_resp: TokenResponse = serde_json::from_slice(&bytes).map_err(Error::Json)?;
    Ok(token_resp.access_token.into())
}

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(TIMEOUT))
        .max_redirects(0)
        .http_status_as_error(false)
        .user_agent("Herdr-GPUI")
        .build()
        .into()
}

fn authorization(key: &SecretString) -> Result<ureq::http::HeaderValue> {
    let mut text = SecretBox::new(Box::new(String::with_capacity(
        7 + key.expose_secret().len(),
    )));
    text.expose_secret_mut().push_str("Bearer ");
    text.expose_secret_mut().push_str(key.expose_secret());
    let mut value =
        ureq::http::HeaderValue::from_str(text.expose_secret()).map_err(Error::Header)?;
    value.set_sensitive(true);
    Ok(value)
}

fn network(context: &'static str) -> impl FnOnce(ureq::Error) -> Error {
    move |error| {
        tracing::warn!(category = "azure_http", context, error = %error, "Azure request failed");
        Error::Network(error)
    }
}

fn read_body(response: &mut ureq::http::Response<ureq::Body>) -> Result<Zeroizing<Vec<u8>>> {
    let mut bytes = Zeroizing::new(Vec::with_capacity(64 * 1024));
    response
        .body_mut()
        .as_reader()
        .take(LIMIT + 1)
        .read_to_end(&mut bytes)
        .map_err(Error::Read)?;
    if bytes.len() as u64 > LIMIT {
        return Err(Error::Size);
    }
    Ok(bytes)
}

/// Inner errors kept in a diagnostic; Azure lists one per failing resource.
const DETAILS: usize = 4;

/// Azure's message for an error body, followed by its inner errors' own, which
/// say what went wrong when the outer one only says to see them.
pub(crate) fn diagnostic(bytes: &[u8]) -> String {
    let Ok(value) = serde_json::from_slice::<serde_json::Value>(bytes) else {
        return String::from_utf8_lossy(bytes).trim().to_owned();
    };
    let error = value.get("error").unwrap_or(&value);
    let Some(message) = error.get("message").and_then(|v| v.as_str()) else {
        return String::from_utf8_lossy(bytes).trim().to_owned();
    };
    let mut text = message.trim().to_owned();
    for detail in details(error).take(DETAILS) {
        if let Some(inner) = detail.get("message").and_then(|v| v.as_str()) {
            text.push_str(" — ");
            text.push_str(inner.trim());
        }
    }
    text
}

/// The `code` of an error body and of each inner error.
pub(super) fn codes(bytes: &[u8]) -> Vec<String> {
    let Ok(value) = serde_json::from_slice::<serde_json::Value>(bytes) else {
        return Vec::new();
    };
    let error = value.get("error").unwrap_or(&value);
    std::iter::once(error)
        .chain(details(error))
        .filter_map(|error| error.get("code")?.as_str().map(str::to_owned))
        .collect()
}

fn details(error: &serde_json::Value) -> impl Iterator<Item = &serde_json::Value> {
    error
        .get("details")
        .and_then(|v| v.as_array())
        .into_iter()
        .flatten()
}
