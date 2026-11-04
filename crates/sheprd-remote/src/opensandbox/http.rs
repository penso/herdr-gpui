//! Bounded HTTP against an OpenSandbox server and a sandbox's execd: a
//! size-capped reader, redirects refused, the API key as a sensitive header,
//! and failures reduced to one short line so a hostile server cannot flood
//! the UI or the log. The same policy as the other providers' clients.

use super::{Error, Result, Status};
use secrecy::{ExposeSecret, SecretString};
use serde::{Serialize, de::DeserializeOwned};
use std::{collections::BTreeMap, io::Read, time::Duration};

const LIMIT: u64 = 4 * 1024 * 1024;
const MESSAGE_LIMIT: usize = 300;
const TIMEOUT: Duration = Duration::from_secs(30);
/// The header the lifecycle API authenticates by.
pub const KEY_HEADER: &str = "OPEN-SANDBOX-API-KEY";

/// Headers a request carries: the API key, or what an endpoint requires.
pub enum Auth<'a> {
    Key(&'a SecretString),
    Endpoint(&'a BTreeMap<String, String>),
}

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(TIMEOUT))
        .max_redirects(0)
        .http_status_as_error(false)
        .user_agent("Sheprd")
        .build()
        .into()
}

fn network(context: &'static str) -> impl FnOnce(ureq::Error) -> Error {
    move |error| {
        tracing::warn!(category = "opensandbox_http", context, error = %error, "OpenSandbox request failed");
        Error::Network(error)
    }
}

fn body(response: &mut ureq::http::Response<ureq::Body>) -> Result<Vec<u8>> {
    let mut bytes = Vec::with_capacity(16 * 1024);
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

/// One short line from an error reply's `message`, `detail`, or `error`.
pub(super) fn message(bytes: &[u8]) -> Option<String> {
    #[derive(serde::Deserialize)]
    struct Reply {
        message: Option<String>,
        detail: Option<serde_json::Value>,
        error: Option<String>,
    }
    let reply = serde_json::from_slice::<Reply>(bytes).ok()?;
    let detail = reply.detail.and_then(|detail| match detail {
        serde_json::Value::String(text) => Some(text),
        serde_json::Value::Object(object) => object
            .get("message")
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned),
        _ => None,
    });
    let text: String = reply
        .message
        .or(detail)
        .or(reply.error)?
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .take(MESSAGE_LIMIT)
        .collect();
    let text = text.trim();
    (!text.is_empty()).then(|| text.to_owned())
}

pub(super) fn failure(context: &'static str, status: u16, bytes: &[u8]) -> Error {
    tracing::warn!(
        category = "opensandbox_http",
        context,
        status,
        "OpenSandbox request failed"
    );
    match status {
        401 => Error::Authentication,
        403 => Error::Forbidden,
        code => Status {
            code,
            message: message(bytes),
        }
        .into(),
    }
}

fn checked(
    context: &'static str,
    mut response: ureq::http::Response<ureq::Body>,
) -> Result<Vec<u8>> {
    let status = response.status().as_u16();
    let bytes = body(&mut response)?;
    if !(200..=299).contains(&status) {
        return Err(failure(context, status, &bytes));
    }
    Ok(bytes)
}

fn decode<T: DeserializeOwned>(bytes: &[u8]) -> Result<T> {
    serde_json::from_slice(bytes).map_err(Error::Json)
}

fn authorize<B>(request: ureq::RequestBuilder<B>, auth: &Auth) -> Result<ureq::RequestBuilder<B>> {
    let request = request.header("Accept", "application/json");
    Ok(match auth {
        Auth::Key(key) => {
            let mut value =
                ureq::http::HeaderValue::from_str(key.expose_secret()).map_err(Error::Header)?;
            value.set_sensitive(true);
            request.header(KEY_HEADER, value)
        }
        Auth::Endpoint(headers) => headers.iter().try_fold(request, |request, (name, value)| {
            let mut value = ureq::http::HeaderValue::from_str(value).map_err(Error::Header)?;
            value.set_sensitive(true);
            Ok::<_, Error>(request.header(name.as_str(), value))
        })?,
    })
}

pub fn get<T: DeserializeOwned>(context: &'static str, url: &str, auth: &Auth) -> Result<T> {
    let reply = authorize(agent().get(url), auth)?
        .call()
        .map_err(network(context))?;
    decode(&checked(context, reply)?)
}

pub fn post<T: DeserializeOwned>(
    context: &'static str,
    url: &str,
    auth: &Auth,
    payload: &impl Serialize,
) -> Result<T> {
    let body = serde_json::to_vec(payload).map_err(Error::Json)?;
    let reply = authorize(agent().post(url), auth)?
        .header("Content-Type", "application/json")
        .send(&body[..])
        .map_err(network(context))?;
    decode(&checked(context, reply)?)
}

/// A POST whose reply body is not needed, such as resuming a sandbox.
pub fn post_empty(context: &'static str, url: &str, auth: &Auth) -> Result<()> {
    let reply = authorize(agent().post(url), auth)?
        .send_empty()
        .map_err(network(context))?;
    checked(context, reply).map(drop)
}

/// A DELETE; a resource already gone is not an error.
pub fn delete(context: &'static str, url: &str, auth: &Auth) -> Result<()> {
    let reply = authorize(agent().delete(url), auth)?
        .call()
        .map_err(network(context))?;
    match checked(context, reply) {
        Ok(_) | Err(Error::Status(Status { code: 404, .. })) => Ok(()),
        Err(error) => Err(error),
    }
}
