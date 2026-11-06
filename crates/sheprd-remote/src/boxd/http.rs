//! Bounded unary calls to boxd's public API as grpc-web over HTTP/1.1: the
//! same policy as the other providers' REST clients (a size-capped reader,
//! redirects refused, a sensitive bearer header, failures reduced to one short
//! line), with the reply's gRPC status read from its trailer frame or, for a
//! refusal before any message, from its headers. The API key travels as the
//! bearer itself, which boxd accepts over TLS (and on loopback) without an
//! exchange for a session token.

use super::{
    Error, Result, Settings,
    error::{Code, Status},
    wire::{self, TRAILER, Trailers},
};
use secrecy::{ExposeSecret, ExposeSecretMut, SecretBox, SecretString};
use std::{io::Read, time::Duration};

const LIMIT: u64 = 4 * 1024 * 1024;
const MESSAGE_LIMIT: usize = 300;
const TIMEOUT: Duration = Duration::from_secs(30);
/// The gRPC service every call names.
pub const SERVICE: &str = "/boxd.api.v1.BoxdApi/";
pub const CONTENT_TYPE: &str = "application/grpc-web+proto";

/// gRPC status codes this app tells apart.
pub const NOT_FOUND: u32 = 5;
pub const PERMISSION_DENIED: u32 = 7;
pub const UNAUTHENTICATED: u32 = 16;

/// One authenticated API call. Requests go to the configured API origin only.
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

/// The `Authorization` header for `key`, marked sensitive.
pub fn authorization(key: &SecretString) -> Result<ureq::http::HeaderValue> {
    let mut text = SecretBox::new(Box::new(String::with_capacity(
        7 + key.expose_secret().len(),
    )));
    text.expose_secret_mut().push_str("Bearer ");
    text.expose_secret_mut().push_str(key.expose_secret());
    let mut header =
        ureq::http::HeaderValue::from_str(text.expose_secret()).map_err(Error::Header)?;
    header.set_sensitive(true);
    Ok(header)
}

/// One short, control-free line of a status message.
pub(super) fn bounded(text: &str) -> Option<String> {
    let text: String = text
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .take(MESSAGE_LIMIT)
        .collect();
    let text = text.trim();
    (!text.is_empty()).then(|| text.to_owned())
}

/// The error for a non-zero gRPC status.
pub(super) fn failure(context: &'static str, trailers: &Trailers) -> Error {
    let code = trailers.status.unwrap_or(2);
    tracing::warn!(category = "boxd_http", context, code, "boxd request failed");
    match code {
        UNAUTHENTICATED => Error::Authentication,
        PERMISSION_DENIED => Error::Forbidden,
        code => Status {
            code: Code::Grpc(code),
            message: trailers.message.as_deref().and_then(bounded),
        }
        .into(),
    }
}

/// The one message of a grpc-web reply `body`, or the status it failed with.
pub(super) fn reply(context: &'static str, body: &[u8]) -> Result<Vec<u8>> {
    let mut rest = body;
    let mut message = None;
    while let Some((flag, payload, used)) = wire::take_frame(rest)? {
        rest = &rest[used..];
        if flag & TRAILER != 0 {
            let trailers = Trailers::parse(payload);
            return match trailers.status {
                Some(0) => message.ok_or(Error::Wire),
                _ => Err(failure(context, &trailers)),
            };
        }
        message = Some(payload.to_vec());
    }
    Err(Error::Wire)
}

impl Request<'_> {
    /// Call `method` with the encoded request `message`; returns the encoded
    /// reply.
    pub fn call(&self, context: &'static str, method: &str, message: &[u8]) -> Result<Vec<u8>> {
        let url = format!("{}{SERVICE}{method}", self.settings.base);
        let mut response = agent()
            .post(url)
            .header("Content-Type", CONTENT_TYPE)
            .header("Accept", CONTENT_TYPE)
            .header("X-Grpc-Web", "1")
            .header("Authorization", authorization(self.key)?)
            .send(&wire::frame(message)[..])
            .map_err(|error| {
                tracing::warn!(category = "boxd_http", context, error = %error, "boxd request failed");
                Error::Network(error)
            })?;
        let status = response.status().as_u16();
        // A refusal before any message is a reply of headers alone.
        let mut trailers = Trailers::default();
        for (name, value) in response.headers() {
            if let Ok(value) = value.to_str() {
                trailers.header(name.as_str(), value);
            }
        }
        match trailers.status {
            Some(0) | None => {}
            Some(_) => return Err(failure(context, &trailers)),
        }
        if status != 200 {
            tracing::warn!(
                category = "boxd_http",
                context,
                status,
                "boxd request failed"
            );
            return Err(match status {
                401 => Error::Authentication,
                403 => Error::Forbidden,
                code => Status {
                    code: Code::Http(code),
                    message: None,
                }
                .into(),
            });
        }
        let mut bytes = Vec::with_capacity(4 * 1024);
        response
            .body_mut()
            .as_reader()
            .take(LIMIT + 1)
            .read_to_end(&mut bytes)
            .map_err(Error::Read)?;
        if bytes.len() as u64 > LIMIT {
            return Err(Error::Size);
        }
        reply(context, &bytes)
    }
}
