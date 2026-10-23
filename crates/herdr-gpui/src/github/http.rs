//! Bounded HTTP against GitHub: a size-capped reader, redacted authorization
//! headers, the GraphQL call, and the account-wide rate-limit cooldown. Remote
//! diagnostics stay bounded so a hostile response cannot flood the UI.

use super::{Error, Result, log};
use crate::updater::{quota_exhausted, rate_limit_reset};
use chrono::{DateTime, Utc};
use secrecy::{ExposeSecret, ExposeSecretMut, SecretBox as Secret, SecretString};
use serde::de::DeserializeOwned;
use serde_json::Value;
use std::{
    io::Read,
    time::{Duration, Instant},
};
use zeroize::Zeroizing;

pub(super) const LIMIT: u64 = 2 * 1024 * 1024;

pub(super) fn agent(timeout: Duration) -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(timeout))
        .max_redirects(0)
        .http_status_as_error(false)
        .build()
        .into()
}

pub(super) fn response<T: DeserializeOwned>(
    context: &'static str,
    mut response: ureq::http::Response<ureq::Body>,
) -> Result<T> {
    let status = response.status().as_u16();
    if !(200..=299).contains(&status) {
        log::http(context, status, response.headers());
        let headers = response.headers();
        return Err(match status {
            401 => Error::Authentication,
            403 if !quota_exhausted(headers) => Error::Forbidden,
            403 | 429 => Error::RateLimit {
                reset: rate_limit_reset(headers, Utc::now()),
            },
            status => Error::Status(status),
        }
        .into());
    }
    // Allocate the bounded capacity up front: no reallocations leave old body
    // fragments behind, and partial reads are wiped even on I/O errors.
    let mut bytes = Zeroizing::new(Vec::with_capacity(LIMIT as usize + 1));
    response
        .body_mut()
        .as_reader()
        .take(LIMIT + 1)
        .read_to_end(&mut bytes)
        .map_err(Error::Read)?;
    if bytes.len() > LIMIT as usize {
        tracing::warn!(
            category = "github_http",
            context,
            status,
            "GitHub response exceeded the size limit"
        );
        return Err(Error::Size.into());
    }
    Ok(serde_json::from_slice(&bytes).map_err(|error| {
        // The body may hold credentials, so only its shape is recorded.
        tracing::warn!(
            category = "github_http",
            context,
            status,
            bytes = bytes.len() as u64,
            "GitHub response was not the expected JSON"
        );
        Error::json(error)
    })?)
}

pub(crate) fn graphql(
    context: &'static str,
    token: &SecretString,
    query: &str,
    variables: Value,
    timeout: Duration,
    cancelled: impl Fn() -> bool,
    cooldown: &mut Option<Duration>,
) -> Result<Value> {
    let deadline = Instant::now() + timeout;
    if cancelled() {
        return Err(crate::pull_request::Error::Cancelled.into());
    }
    let repository = requested_repository(&variables);
    let (result, exchange) = post(token, query, variables, deadline, cooldown)?;
    if cancelled() {
        return Err(crate::pull_request::Error::Cancelled.into());
    }
    if let Some(errors) = result.get("errors") {
        let kind = log::token_kind(token);
        log::graphql(context, kind, &exchange, errors);
        if errors.as_array().is_some_and(|errors| {
            errors.iter().any(|error| {
                matches!(
                    error["type"].as_str(),
                    Some("RATE_LIMITED" | "FORBIDDEN" | "UNAUTHORIZED")
                )
            })
        }) {
            *cooldown = Some(pause_until(exchange.reset, Utc::now()));
        }
        return Err(query_error(errors, repository, kind, exchange.reset).into());
    }
    Ok(result)
}

/// A query of aliased fields, each answered as its own `repository`: an error
/// whose path starts at one alias fails only that one, as GitHub reports a
/// repository it would not show beside ones it did. An exhausted quota, or an
/// error no alias claims, fails the whole request. Pull request lookups are
/// its only caller, so failures are logged under their context.
pub(crate) fn graphql_aliases(
    token: &SecretString,
    query: &str,
    variables: Value,
    aliases: &[(String, String)],
    timeout: Duration,
    cancelled: impl Fn() -> bool,
    cooldown: &mut Option<Duration>,
) -> Result<Vec<Result<Value>>> {
    let deadline = Instant::now() + timeout;
    if cancelled() {
        return Err(crate::pull_request::Error::Cancelled.into());
    }
    let (result, exchange) = post(token, query, variables, deadline, cooldown)?;
    if cancelled() {
        return Err(crate::pull_request::Error::Cancelled.into());
    }
    let kind = log::token_kind(token);
    if let Some(errors) = result.get("errors") {
        log::graphql("pull_request", kind, &exchange, errors);
    }
    split_aliases(result, aliases, kind, exchange.reset, Utc::now(), cooldown)
}

/// Each alias's answer, or the error that fails the whole request. Pure, so
/// the attribution of errors to aliases is testable without GitHub.
pub(super) fn split_aliases(
    mut result: Value,
    aliases: &[(String, String)],
    token_kind: &str,
    reset: Option<DateTime<Utc>>,
    now: DateTime<Utc>,
    cooldown: &mut Option<Duration>,
) -> Result<Vec<Result<Value>>> {
    let errors = result
        .get_mut("errors")
        .map(Value::take)
        .unwrap_or_default();
    let listed = errors.as_array().map(Vec::as_slice).unwrap_or_default();
    let typed = |types: &[&str]| {
        listed.iter().any(|error| {
            error["type"]
                .as_str()
                .is_some_and(|kind| types.contains(&kind))
        })
    };
    if typed(&["RATE_LIMITED", "FORBIDDEN", "UNAUTHORIZED"]) {
        *cooldown = Some(pause_until(reset, now));
    }
    let claimed = |error: &Value| {
        error["path"][0]
            .as_str()
            .is_some_and(|alias| aliases.iter().any(|(known, _)| known == alias))
    };
    if typed(&["RATE_LIMITED"]) || !listed.iter().all(claimed) {
        return Err(query_error(&errors, None, token_kind, reset).into());
    }
    Ok(aliases
        .iter()
        .map(|(alias, repository)| {
            let own: Vec<Value> = listed
                .iter()
                .filter(|error| error["path"][0].as_str() == Some(alias))
                .map(|error| {
                    // Re-rooted so the alias reads as the single query's field.
                    let mut error = error.clone();
                    error["path"][0] = Value::from("repository");
                    error
                })
                .collect();
            if own.is_empty() {
                let answer = result["data"][alias.as_str()].take();
                Ok(serde_json::json!({"data": {"repository": answer}}))
            } else {
                Err(query_error(
                    &Value::Array(own),
                    Some(repository.clone()),
                    token_kind,
                    reset,
                )
                .into())
            }
        })
        .collect())
}

/// A GraphQL mutation the user asked for. It is sent once and never retried
/// or cancelled midway; a refusal carries GitHub's own reason (cleaned and
/// bounded) because "the branch moved" or "not mergeable" is the answer the
/// user needs, and a generic failure would invite a blind retry.
pub(crate) fn mutation(
    context: &'static str,
    token: &SecretString,
    query: &str,
    variables: Value,
    timeout: Duration,
) -> Result<Value> {
    // No account-wide cooldown: nothing retries a mutation to honour one.
    let repository = requested_repository(&variables);
    let (result, exchange) = post(token, query, variables, Instant::now() + timeout, &mut None)?;
    if let Some(errors) = result.get("errors") {
        let kind = log::token_kind(token);
        log::graphql(context, kind, &exchange, errors);
        return Err(query_error(errors, repository, kind, exchange.reset).into());
    }
    Ok(result)
}

/// A REST read of `path` under `https://api.github.com`, such as
/// `/repos/OWNER/REPO/security-advisories`, through the same bounded reader
/// and typed refusals as GraphQL. Callers build `path` from names they have
/// validated; nothing here escapes it. The answer is read straight into
/// `T`, so fields `T` leaves out are skipped rather than held.
pub(crate) fn rest<T: DeserializeOwned>(
    context: &'static str,
    token: &SecretString,
    path: &str,
    timeout: Duration,
    cancelled: impl Fn() -> bool,
) -> Result<T> {
    if cancelled() {
        return Err(crate::pull_request::Error::Cancelled.into());
    }
    let reply = agent(timeout)
        .get(format!("https://api.github.com{path}"))
        .header("User-Agent", "Herdr-GPUI")
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .header("Authorization", authorization(token)?)
        .call()
        .map_err(log::network(context))?;
    if cancelled() {
        return Err(crate::pull_request::Error::Cancelled.into());
    }
    response(context, reply)
}

/// The `owner/repo` a request names, so a refusal can say which repository
/// GitHub would not show. Every repository query here uses these variables.
fn requested_repository(variables: &Value) -> Option<String> {
    let owner = variables.get("owner")?.as_str()?;
    let repo = variables.get("repo")?.as_str()?;
    Some(format!("{owner}/{repo}"))
}

/// What to do when GitHub reports an existing repository as missing. A GitHub
/// App user token only reaches repositories the App is installed on, and no
/// token reaches another account's private repositories, such as those of an
/// Enterprise Managed User, which an additional account can read instead.
pub(crate) fn hidden_hint(app_token: bool) -> &'static str {
    if app_token {
        "This GitHub App sign-in only reaches repositories the App is installed on: install it for the repository's owner and approve the permissions it asks for, or add the account that owns it on the GitHub page."
    } else {
        "Check the remote, the token's repository access, and SSO authorization, or add the account that owns it on the GitHub page."
    }
}

/// Why GitHub answered a GraphQL request with errors, so the user reads the
/// cause and when to retry instead of a generic failure. An exhausted quota
/// wins; then a repository the credential cannot see; then GitHub's own
/// words.
pub(super) fn query_error(
    errors: &Value,
    repository: Option<String>,
    token_kind: &str,
    reset: Option<DateTime<Utc>>,
) -> Error {
    let listed = errors.as_array().map(Vec::as_slice).unwrap_or_default();
    if listed
        .iter()
        .any(|error| error["type"].as_str() == Some("RATE_LIMITED"))
    {
        return Error::RateLimit { reset };
    }
    let hidden = listed.iter().any(|error| {
        error["type"].as_str() == Some("NOT_FOUND")
            && error["path"] == serde_json::json!(["repository"])
    });
    if let Some(repository) = repository.filter(|_| hidden) {
        return Error::RepositoryHidden {
            repository,
            app_token: token_kind == "github_app_user",
        };
    }
    rejection(errors).map_or(Error::Query, Error::Rejected)
}

/// GraphQL endpoints that tests stand in for GitHub, by the token that reads
/// them, so a test's fake never answers another test's calls.
#[cfg(test)]
static FAKE_GRAPHQL: std::sync::Mutex<Vec<(String, String)>> = std::sync::Mutex::new(Vec::new());

/// Sends GraphQL calls made with `token` to `url` instead of GitHub, for
/// the rest of the test process.
#[cfg(test)]
pub(crate) fn fake_graphql(token: &str, url: String) {
    if let Ok(mut fakes) = FAKE_GRAPHQL.lock() {
        fakes.retain(|(fake, _)| fake != token);
        fakes.push((token.to_owned(), url));
    }
}

fn graphql_url(token: &SecretString) -> String {
    #[cfg(test)]
    if let Some((_, url)) = FAKE_GRAPHQL.lock().ok().and_then(|fakes| {
        fakes
            .iter()
            .find(|(fake, _)| fake == token.expose_secret())
            .cloned()
    }) {
        return url;
    }
    #[cfg(not(test))]
    let _ = token;
    "https://api.github.com/graphql".to_owned()
}

/// One GraphQL POST, its bounded JSON body, and the headers that explain a
/// failure GraphQL reports with a 200.
fn post(
    token: &SecretString,
    query: &str,
    variables: Value,
    deadline: Instant,
    cooldown: &mut Option<Duration>,
) -> Result<(Value, log::Exchange)> {
    let body = serde_json::json!({"query":query,"variables":variables}).to_string();
    let timeout = deadline
        .checked_duration_since(Instant::now())
        .ok_or(crate::pull_request::Error::Timeout)?;
    let reply = agent(timeout)
        .post(graphql_url(token))
        .header("User-Agent", "Herdr-GPUI")
        .header("Accept", "application/vnd.github+json")
        .header("Content-Type", "application/json")
        .header("Authorization", authorization(token)?)
        .send(body.as_bytes())
        .map_err(log::network("graphql"))?;
    *cooldown = pr_cooldown(
        reply.status().as_u16(),
        reply.headers(),
        std::time::SystemTime::now(),
    );
    // GraphQL reports failures with a 200, so the headers that explain them are
    // captured before the body consumes the response.
    let exchange = log::Exchange::new(reply.headers());
    let result: Value = response("graphql", reply)?;
    Ok((result, exchange))
}

/// GitHub's first error message, as display text. Remote text is untrusted:
/// controls and bidirectional overrides are blanked and the length bounded.
pub(super) fn rejection(errors: &Value) -> Option<String> {
    let message = errors.get(0)?.get("message")?.as_str()?;
    let message: String = crate::pull_request::clean(message)
        .trim()
        .chars()
        .take(REJECTION_LIMIT)
        .collect();
    (!message.is_empty()).then_some(message)
}

const REJECTION_LIMIT: usize = 240;

pub(super) fn authorization(token: &SecretString) -> Result<ureq::http::HeaderValue> {
    let mut text = Secret::new(Box::new(String::with_capacity(
        7 + token.expose_secret().len(),
    )));
    text.expose_secret_mut().push_str("Bearer ");
    text.expose_secret_mut().push_str(token.expose_secret());
    let mut header =
        ureq::http::HeaderValue::from_str(text.expose_secret()).map_err(Error::Header)?;
    header.set_sensitive(true);
    Ok(header)
}

/// How long an account waits after GraphQL refused it: until the quota
/// refills when GitHub said, else an hour, within the same bounds as
/// [`pr_cooldown`].
pub(super) fn pause_until(reset: Option<DateTime<Utc>>, now: DateTime<Utc>) -> Duration {
    reset
        .and_then(|reset| (reset - now).to_std().ok())
        .map_or(Duration::from_secs(3600), |wait| {
            wait.clamp(Duration::from_secs(300), Duration::from_secs(86400))
        })
}

pub(super) fn pr_cooldown(
    status: u16,
    headers: &ureq::http::HeaderMap,
    now: std::time::SystemTime,
) -> Option<Duration> {
    if !matches!(status, 401 | 403 | 429) {
        return None;
    }
    let seconds = |name| {
        headers
            .get(name)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.parse::<u64>().ok())
    };
    let reset = seconds("x-ratelimit-reset").map(|reset| {
        reset.saturating_sub(
            now.duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
        )
    });
    Some(Duration::from_secs(
        seconds("retry-after")
            .into_iter()
            .chain(reset)
            .max()
            .unwrap_or(3600)
            .clamp(300, 86400),
    ))
}

pub(super) fn oauth<T: DeserializeOwned>(
    path: &'static str,
    fields: &[(&str, &str)],
    timeout: Duration,
) -> Result<T> {
    // ureq owns form serialization and HTTP/TLS buffers; their copies cannot be
    // zeroized by this module. Never log request fields or response bodies.
    response(
        path,
        agent(timeout)
            .post(format!("https://github.com/login/{path}"))
            .header("User-Agent", "Herdr-GPUI")
            .header("Accept", "application/json")
            .send_form(fields.iter().copied())
            .map_err(log::network(path))?,
    )
}
