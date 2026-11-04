//! Bounded HTTP against Boat's API: size-capped reads, redirects refused,
//! sensitive bearer headers, and short sanitized remote diagnostics.

use super::{Error, Result, Settings, Status};
use secrecy::{ExposeSecret, ExposeSecretMut, SecretBox, SecretString};
use serde::{Serialize, de::DeserializeOwned};
use std::{io::Read, time::Duration};
use zeroize::Zeroizing;

const LIMIT: u64 = 4 * 1024 * 1024;
const MESSAGE_LIMIT: usize = 300;
const TIMEOUT: Duration = Duration::from_secs(30);

pub struct Request<'a> {
    pub settings: &'a Settings,
    pub key: &'a SecretString,
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

fn header(value: &str) -> Result<ureq::http::HeaderValue> {
    ureq::http::HeaderValue::from_str(value).map_err(Error::Header)
}

fn authorization(key: &SecretString) -> Result<ureq::http::HeaderValue> {
    let mut text = SecretBox::new(Box::new(String::with_capacity(
        7 + key.expose_secret().len(),
    )));
    text.expose_secret_mut().push_str("Bearer ");
    text.expose_secret_mut().push_str(key.expose_secret());
    let mut value = header(text.expose_secret())?;
    value.set_sensitive(true);
    Ok(value)
}

fn network(context: &'static str) -> impl FnOnce(ureq::Error) -> Error {
    move |error| {
        tracing::warn!(category = "boat_http", context, error = %error, "Boat request failed");
        Error::Network(error)
    }
}

fn body(response: &mut ureq::http::Response<ureq::Body>) -> Result<Zeroizing<Vec<u8>>> {
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

fn message(bytes: &[u8]) -> Option<String> {
    #[derive(serde::Deserialize)]
    struct Reply {
        message: Option<String>,
    }
    let text = serde_json::from_slice::<Reply>(bytes).ok()?.message?;
    let text = diagnostic(&text);
    (!text.is_empty()).then_some(text)
}

pub(super) fn diagnostic(text: &str) -> String {
    let text: String = text
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .take(MESSAGE_LIMIT)
        .collect();
    text.trim().to_owned()
}

fn failure(context: &'static str, status: u16, bytes: &[u8]) -> Error {
    tracing::warn!(
        category = "boat_http",
        context,
        status,
        "Boat request failed"
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
    serde_json::from_slice(&bytes).map_err(|error| {
        tracing::warn!(
            category = "boat_http",
            context,
            status,
            bytes = bytes.len() as u64,
            "Boat response was not the expected JSON"
        );
        Error::Json(error)
    })
}

impl Request<'_> {
    fn url(&self, path: &str) -> String {
        format!("{}{path}", self.settings.base)
    }

    fn call<B>(&self, request: ureq::RequestBuilder<B>) -> Result<ureq::RequestBuilder<B>> {
        let request = request
            .header("Accept", "application/json")
            .header("Authorization", authorization(self.key)?);
        Ok(match &self.settings.organization {
            Some(organization) => request.header("X-Boat-Org", header(organization)?),
            None => request,
        })
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
        self.send(context, self.call(agent().post(self.url(path)))?, payload)
    }

    pub fn post_idempotent<T: DeserializeOwned>(
        &self,
        context: &'static str,
        path: &str,
        idempotency_key: &str,
        payload: &impl Serialize,
    ) -> Result<T> {
        let request = self
            .call(agent().post(self.url(path)))?
            .header("Idempotency-Key", header(idempotency_key)?);
        self.send(context, request, payload)
    }

    pub fn patch<T: DeserializeOwned>(
        &self,
        context: &'static str,
        path: &str,
        payload: &impl Serialize,
    ) -> Result<T> {
        self.send(context, self.call(agent().patch(self.url(path)))?, payload)
    }

    /// A DELETE Boat accepts only with `confirm`, the target's exact ID, in
    /// its confirmation header. The reply body is not needed.
    pub fn delete_confirmed(&self, context: &'static str, path: &str, confirm: &str) -> Result<()> {
        let mut reply = self
            .call(agent().delete(self.url(path)))?
            .header("X-Ascii-Confirm-Delete", header(confirm)?)
            .call()
            .map_err(network(context))?;
        let status = reply.status().as_u16();
        let bytes = body(&mut reply)?;
        if (200..=299).contains(&status) {
            return Ok(());
        }
        Err(failure(context, status, &bytes))
    }

    fn send<T: DeserializeOwned>(
        &self,
        context: &'static str,
        request: ureq::RequestBuilder<ureq::typestate::WithBody>,
        payload: &impl Serialize,
    ) -> Result<T> {
        let bytes = serde_json::to_vec(payload).map_err(Error::Json)?;
        response(
            context,
            request
                .header("Content-Type", "application/json")
                .send(&bytes[..])
                .map_err(network(context))?,
        )
    }
}

#[cfg(test)]
mod tests;
