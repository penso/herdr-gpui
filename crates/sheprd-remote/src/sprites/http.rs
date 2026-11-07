//! Bounded HTTP against the Sprites API: a size-capped reader, redirects
//! refused, a sensitive bearer header, and failures reduced to one short
//! line of the reply so a hostile endpoint cannot flood the UI or the log.
//! The same policy as Daytona's client; only the error type and wording differ.

use super::{Error, Result, Settings, Status};
use secrecy::{ExposeSecret, ExposeSecretMut, SecretBox, SecretString};
use serde::{Serialize, de::DeserializeOwned};
use std::{io::Read, time::Duration};

const LIMIT: u64 = 4 * 1024 * 1024;
const MESSAGE_LIMIT: usize = 300;
const TIMEOUT: Duration = Duration::from_secs(30);

/// One authenticated API call. Requests go to the configured API root only.
pub struct Request<'a> {
    pub settings: &'a Settings,
    pub token: &'a SecretString,
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

/// The `Authorization` header for `token`, marked sensitive.
pub fn authorization(token: &SecretString) -> Result<ureq::http::HeaderValue> {
    let mut text = SecretBox::new(Box::new(String::with_capacity(
        7 + token.expose_secret().len(),
    )));
    text.expose_secret_mut().push_str("Bearer ");
    text.expose_secret_mut().push_str(token.expose_secret());
    let mut header =
        ureq::http::HeaderValue::from_str(text.expose_secret()).map_err(Error::Header)?;
    header.set_sensitive(true);
    Ok(header)
}

fn network(context: &'static str) -> impl FnOnce(ureq::Error) -> Error {
    move |error| {
        tracing::warn!(category = "sprites_http", context, error = %error, "Sprites request failed");
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

/// One short line from an error reply's `error` or `message`.
pub(super) fn message(bytes: &[u8]) -> Option<String> {
    #[derive(serde::Deserialize)]
    struct Reply {
        error: Option<String>,
        message: Option<String>,
    }
    let reply = serde_json::from_slice::<Reply>(bytes).ok()?;
    let text: String = reply
        .error
        .or(reply.message)?
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .take(MESSAGE_LIMIT)
        .collect();
    let text = text.trim();
    (!text.is_empty()).then(|| text.to_owned())
}

pub(super) fn failure(context: &'static str, status: u16, bytes: &[u8]) -> Error {
    tracing::warn!(
        category = "sprites_http",
        context,
        status,
        "Sprites request failed"
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

fn response<T: DeserializeOwned>(
    context: &'static str,
    mut response: ureq::http::Response<ureq::Body>,
) -> Result<T> {
    let status = response.status().as_u16();
    let bytes = body(&mut response)?;
    if !(200..=299).contains(&status) {
        return Err(failure(context, status, &bytes));
    }
    serde_json::from_slice(&bytes).map_err(Error::Json)
}

/// A reply whose body is not needed.
fn empty(context: &'static str, mut reply: ureq::http::Response<ureq::Body>) -> Result<u16> {
    let status = reply.status().as_u16();
    if (200..=299).contains(&status) {
        return Ok(status);
    }
    let bytes = body(&mut reply)?;
    Err(failure(context, status, &bytes))
}

impl Request<'_> {
    fn url(&self, path: &str) -> String {
        format!("{}{path}", self.settings.base)
    }

    fn call<B>(&self, request: ureq::RequestBuilder<B>) -> Result<ureq::RequestBuilder<B>> {
        Ok(request
            .header("Accept", "application/json")
            .header("Authorization", authorization(self.token)?))
    }

    pub fn get<T: DeserializeOwned>(&self, context: &'static str, path: &str) -> Result<T> {
        response(
            context,
            self.call(agent().get(self.url(path)))?
                .call()
                .map_err(network(context))?,
        )
    }

    pub fn post<T: DeserializeOwned>(
        &self,
        context: &'static str,
        path: &str,
        payload: &impl Serialize,
    ) -> Result<T> {
        let body = serde_json::to_vec(payload).map_err(Error::Json)?;
        response(
            context,
            self.call(agent().post(self.url(path)))?
                .header("Content-Type", "application/json")
                .send(&body[..])
                .map_err(network(context))?,
        )
    }

    /// A PUT whose reply body is not needed, such as registering a service.
    pub fn put(&self, context: &'static str, path: &str, payload: &impl Serialize) -> Result<()> {
        let body = serde_json::to_vec(payload).map_err(Error::Json)?;
        empty(
            context,
            self.call(agent().put(self.url(path)))?
                .header("Content-Type", "application/json")
                .send(&body[..])
                .map_err(network(context))?,
        )
        .map(drop)
    }

    /// A DELETE; a resource already gone is not an error.
    pub fn delete(&self, context: &'static str, path: &str) -> Result<()> {
        match empty(
            context,
            self.call(agent().delete(self.url(path)))?
                .call()
                .map_err(network(context))?,
        ) {
            Ok(_) | Err(Error::Status(Status { code: 404, .. })) => Ok(()),
            Err(error) => Err(error),
        }
    }
}
