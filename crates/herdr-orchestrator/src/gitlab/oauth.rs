//! Connecting a GitLab account with OAuth 2.0's device authorization grant
//! (RFC 8628), as GitHub's sign-in does: GitLab shows a code, the user
//! approves it in the browser, and the app receives an access token with a
//! refresh token that renews it. The application is public, so only its ID
//! is sent; it has no secret to keep. Every call blocks on the network:
//! run it from a worker.

use super::{Error, TIMEOUT, agent, body, saved::OAuth, valid};
use secrecy::{ExposeSecret, SecretString};
use serde::Deserialize;
use std::time::Duration;

/// Herdr UI's OAuth application on gitlab.com. Public by design: a device
/// flow application has no secret.
pub const GITLAB_COM_APPLICATION: &str =
    "8ac761647c83bb908a4ceabecdcf4ff2379a7dab1800c9674d55b480793ed9ef";
/// What a connected account may do: read issues and whom it signs in as.
pub const SCOPE: &str = "read_api";
/// GitLab's answers are small objects; anything larger is refused.
const LIMIT: u64 = 64 * 1024;
/// How long an access token lasts when GitLab's reply leaves it out.
const DEFAULT_LIFETIME: u64 = 2 * 60 * 60;
/// The longest a code is waited for, whatever GitLab says.
const MAX_WAIT: u64 = 30 * 60;

/// A code GitLab issued, waiting for the user to approve it.
pub struct Device {
    pub device_code: SecretString,
    /// What the user types or checks on GitLab's page.
    pub user_code: String,
    /// GitLab's approval page, with the code filled in when GitLab offers it.
    pub open: url::Url,
    pub expires_in: Duration,
    pub interval: Duration,
}

/// Where a device flow stands after one poll.
pub enum Poll {
    Pending,
    /// GitLab asks for polls further apart.
    SlowDown,
    Granted(OAuth),
}

#[derive(Deserialize)]
struct DeviceReply {
    device_code: String,
    user_code: String,
    verification_uri: String,
    verification_uri_complete: Option<String>,
    expires_in: Option<u64>,
    interval: Option<u64>,
}

#[derive(Deserialize)]
struct TokenReply {
    access_token: String,
    refresh_token: Option<String>,
    expires_in: Option<u64>,
}

#[derive(Deserialize)]
struct ErrorReply {
    error: String,
}

/// Asks GitLab at `instance` for a code to connect an account with
/// `application`.
pub fn authorize(instance: &str, application: &str) -> Result<Device, Error> {
    let (status, body) = post(
        instance,
        "authorize_device",
        &[("client_id", application), ("scope", SCOPE)],
    )?;
    parse_device(status, &body, instance)
}

/// Asks once whether the user approved `device`. `now` is seconds since the
/// epoch, which the granted token's expiry counts from.
pub fn poll(instance: &str, application: &str, device: &Device, now: u64) -> Result<Poll, Error> {
    let (status, body) = post(
        instance,
        "token",
        &[
            ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
            ("device_code", device.device_code.expose_secret()),
            ("client_id", application),
        ],
    )?;
    parse_token(status, &body, application, now)
}

/// Trades `kept`'s refresh token for a new pair. GitLab invalidates the old
/// pair, so the caller saves the new one before using it.
pub fn renew(instance: &str, kept: &OAuth, now: u64) -> Result<OAuth, Error> {
    let (status, body) = post(
        instance,
        "token",
        &[
            ("grant_type", "refresh_token"),
            ("refresh_token", kept.refresh.expose_secret()),
            ("client_id", &kept.application),
        ],
    )?;
    match parse_token(status, &body, &kept.application, now)? {
        Poll::Granted(renewed) => Ok(renewed),
        Poll::Pending | Poll::SlowDown => Err(Error::Status(status)),
    }
}

/// A device code reply. The page it names is opened in the browser, so it
/// must be an HTTPS page on the instance asked, never another site.
pub fn parse_device(status: u16, body: &[u8], instance: &str) -> Result<Device, Error> {
    if status != 200 {
        return Err(refusal(status, body));
    }
    let reply: DeviceReply = serde_json::from_slice(body).map_err(Error::Json)?;
    let on_instance = |address: &str| {
        url::Url::parse(address).ok().filter(|url| {
            let host = url.host_str().map(|host| match url.port() {
                Some(port) => format!("{host}:{port}"),
                None => host.to_owned(),
            });
            url.scheme() == "https" && host.is_some_and(|host| host.eq_ignore_ascii_case(instance))
        })
    };
    let open = reply
        .verification_uri_complete
        .as_deref()
        .and_then(on_instance)
        .or_else(|| on_instance(&reply.verification_uri))
        .ok_or_else(|| Error::Address(instance.to_owned()))?;
    let user_code = reply.user_code.trim();
    let device_code = valid(reply.device_code.trim()).ok_or(Error::Token)?;
    if user_code.is_empty()
        || user_code.len() > 64
        || !user_code.bytes().all(|byte| byte.is_ascii_graphic())
    {
        return Err(Error::Token);
    }
    Ok(Device {
        device_code,
        user_code: user_code.to_owned(),
        open,
        expires_in: Duration::from_secs(reply.expires_in.unwrap_or(900).clamp(60, MAX_WAIT)),
        interval: Duration::from_secs(reply.interval.unwrap_or(5).clamp(1, 60)),
    })
}

/// A token endpoint reply: a granted pair, or why there is none yet.
pub fn parse_token(status: u16, body: &[u8], application: &str, now: u64) -> Result<Poll, Error> {
    if status != 200 {
        return match error_code(body).as_deref() {
            Some("authorization_pending") => Ok(Poll::Pending),
            Some("slow_down") => Ok(Poll::SlowDown),
            _ => Err(refusal(status, body)),
        };
    }
    let reply: TokenReply = serde_json::from_slice(body).map_err(Error::Json)?;
    let (Some(access), Some(refresh)) = (
        valid(reply.access_token.trim()),
        reply
            .refresh_token
            .as_deref()
            .and_then(|token| valid(token.trim())),
    ) else {
        return Err(Error::Token);
    };
    let lifetime = reply.expires_in.unwrap_or(DEFAULT_LIFETIME);
    Ok(Poll::Granted(OAuth {
        access,
        refresh,
        expires_at: now.saturating_add(lifetime),
        application: application.to_owned(),
    }))
}

fn error_code(body: &[u8]) -> Option<String> {
    serde_json::from_slice::<ErrorReply>(body)
        .ok()
        .map(|reply| reply.error)
}

/// What a refused OAuth request means.
fn refusal(status: u16, body: &[u8]) -> Error {
    match (status, error_code(body).as_deref()) {
        (_, Some("access_denied")) => Error::Denied,
        (_, Some("expired_token")) => Error::CodeExpired,
        (_, Some("invalid_grant" | "invalid_client" | "unauthorized_client")) | (401, _) => {
            Error::Authentication
        }
        (429, _) => Error::RateLimit,
        (status, _) => Error::Status(status),
    }
}

fn post(instance: &str, path: &str, fields: &[(&str, &str)]) -> Result<(u16, Vec<u8>), Error> {
    let origin = format!("https://{instance}");
    let url = url::Url::parse(&format!("{origin}/oauth/{path}"))
        .map_err(|_| Error::Address(instance.to_owned()))?;
    // ureq owns the form's buffers; never log the fields or the reply.
    let mut reply = agent(TIMEOUT.min(Duration::from_secs(15)))
        .post(url.as_str())
        .header("User-Agent", "Herdr-GPUI")
        .header("Accept", "application/json")
        .send_form(fields.iter().copied())
        .map_err(|error| Error::Network {
            origin,
            source: Box::new(error),
        })?;
    let status = reply.status().as_u16();
    Ok((status, body(&mut reply, LIMIT)?))
}
