//! The credential an Azure DevOps read carries: a personal access token from
//! `AZURE_DEVOPS_EXT_PAT`, the variable the Azure DevOps CLI extension reads,
//! else a Microsoft Entra token the Azure CLI's own sign-in issues for Azure
//! DevOps. Neither is stored by this app.

use super::Error;
use base64::Engine;
use secrecy::{ExposeSecret, SecretString};
use std::{
    ffi::OsString,
    time::{Duration, Instant},
};
use zeroize::Zeroizing;

/// The variable the Azure DevOps CLI extension reads a personal access token from.
pub const PAT_VARIABLE: &str = "AZURE_DEVOPS_EXT_PAT";
/// Azure DevOps's resource id, which an Entra token must be issued for.
const RESOURCE: &str = "499b84ac-1321-427f-aa17-267ca6aa798d";
const CLI_TIMEOUT: Duration = Duration::from_secs(20);
/// An access token is a few kilobytes; the rest is room for warnings.
const CLI_OUTPUT_LIMIT: usize = 64 * 1024;

/// A credential and how Azure DevOps expects it.
pub enum Token {
    /// A personal access token, sent as Basic with an empty user name.
    Personal(SecretString),
    /// An Entra access token, sent as Bearer.
    Entra(SecretString),
}

impl Token {
    /// The `Authorization` header carrying this token, marked sensitive so
    /// it never reaches a log.
    pub fn authorization(&self) -> Result<ureq::http::HeaderValue, Error> {
        let text = match self {
            Self::Personal(token) => {
                let pair = Zeroizing::new(format!(":{}", token.expose_secret()));
                let encoded = Zeroizing::new(
                    base64::engine::general_purpose::STANDARD.encode(pair.as_bytes()),
                );
                Zeroizing::new(format!("Basic {}", encoded.as_str()))
            }
            Self::Entra(token) => Zeroizing::new(format!("Bearer {}", token.expose_secret())),
        };
        let mut header = ureq::http::HeaderValue::from_str(&text).map_err(|_| Error::Token)?;
        header.set_sensitive(true);
        Ok(header)
    }
}

/// The token to read Azure DevOps as: the environment's personal access
/// token, else the Azure CLI's sign-in. Runs a subprocess, which `cancelled`
/// stops: call it from a worker.
pub fn lookup(cancelled: &impl Fn() -> bool) -> Option<Token> {
    environment(herdr_client::login_env::var).or_else(|| cli(cancelled))
}

/// `AZURE_DEVOPS_EXT_PAT`, read through `variable`.
pub fn environment(variable: impl Fn(&str) -> Option<OsString>) -> Option<Token> {
    variable(PAT_VARIABLE)
        .and_then(|value| value.into_string().ok())
        .and_then(|value| valid(value.trim()))
        .map(Token::Personal)
}

/// `az account get-access-token` for Azure DevOps, under the bounded process
/// policy shared with pull request lookups.
fn cli(cancelled: &impl Fn() -> bool) -> Option<Token> {
    let mut command = herdr_client::new_command("az");
    // An app opened from a desktop launcher has a minimal PATH.
    herdr_client::login_env::apply(&mut command);
    command.args([
        "account",
        "get-access-token",
        "--resource",
        RESOURCE,
        "--query",
        "accessToken",
        "--output",
        "tsv",
    ]);
    let result = herdr_github::pull_request::run_bytes(
        &mut command,
        Instant::now() + CLI_TIMEOUT,
        cancelled,
        CLI_OUTPUT_LIMIT,
    );
    match result {
        Ok((true, output)) => parse_cli(&output).map(Token::Entra),
        Ok((false, _)) => None,
        Err(error) => {
            tracing::debug!(
                category = "orchestrator_azure",
                error = %error,
                "The Azure CLI could not be asked for a token"
            );
            None
        }
    }
}

/// The CLI prints the token alone, or nothing when it is signed out.
pub fn parse_cli(output: &[u8]) -> Option<SecretString> {
    let mut lines = std::str::from_utf8(output)
        .ok()?
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty());
    match (lines.next(), lines.next()) {
        (Some(token), None) => valid(token),
        _ => None,
    }
}

/// A token is a run of visible ASCII; anything else would corrupt the header
/// or is not a token at all. Entra tokens run to a few kilobytes.
fn valid(token: &str) -> Option<SecretString> {
    (!token.is_empty()
        && token.len() <= 16 * 1024
        && token.bytes().all(|byte| byte.is_ascii_graphic()))
    .then(|| token.into())
}
