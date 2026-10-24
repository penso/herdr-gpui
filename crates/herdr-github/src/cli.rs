//! The GitHub CLI's own sign-in, read with `gh auth token` for github.com.
//!
//! This app never saves that token. Choosing the GitHub CLI saves only a
//! marker in the credential store, and every load asks `gh` again, so `gh auth
//! logout` or a refreshed token takes effect at the next session check. The
//! token arrives on a pipe, never an argument, and is never logged or shown.

use super::{Profile, Result, profile_from, valid_token};
use crate::pull_request;
use secrecy::SecretString;
use std::{
    process::Command,
    sync::Arc,
    time::{Duration, Instant},
};
use zeroize::Zeroizing;

const TIMEOUT: Duration = Duration::from_secs(10);
/// A token is at most a few hundred bytes; the rest is room for warnings.
const OUTPUT_LIMIT: usize = 16 * 1024;

/// The GitHub CLI's token for github.com. Blocks on a subprocess: call it
/// from a background worker.
pub(super) fn token() -> Result<SecretString> {
    let mut command = herdr_client::new_command("gh");
    // An app opened from the Dock or a desktop launcher has a minimal PATH.
    herdr_client::login_env::apply(&mut command);
    run(&mut command)
}

/// The GitHub CLI's account, read with its token.
pub(super) fn profile() -> Result<Profile> {
    verify(token()?)
}

/// `token`'s account. GitHub refusing it means `gh` holds an expired or
/// revoked token, which only `gh` can replace.
pub(super) fn verify(token: SecretString) -> Result<Profile> {
    profile_from(Arc::new(token), true).map_err(|error| match error {
        crate::Error::Authentication => Error::Rejected.into(),
        error => error,
    })
}

/// `gh auth token` for github.com, under the bounded process policy shared
/// with PR lookups: a deadline, capped output, no prompt, and no
/// `GH_TOKEN` / `GITHUB_TOKEN`, so the answer is `gh`'s own saved sign-in.
pub(super) fn run(command: &mut Command) -> Result<SecretString> {
    command.args(["auth", "token", "--hostname", "github.com"]);
    let (ok, output) =
        pull_request::run_bytes(command, Instant::now() + TIMEOUT, &|| false, OUTPUT_LIMIT)
            .map_err(process_error)?;
    parse(ok, &Zeroizing::new(output))
}

/// `gh` exits nonzero when it holds no token for the host. Its output merges
/// stdout and stderr, so the token is the one line shaped like one.
pub(super) fn parse(ok: bool, output: &[u8]) -> Result<SecretString> {
    if !ok {
        return Err(Error::SignedOut.into());
    }
    let mut tokens = output
        .split(|byte| *byte == b'\n')
        .map(|line| line.strip_suffix(b"\r").unwrap_or(line))
        .filter_map(|line| std::str::from_utf8(line).ok())
        .filter(|line| valid_token(line));
    match (tokens.next(), tokens.next()) {
        (Some(token), None) => Ok(token.into()),
        (None, _) => Err(Error::SignedOut.into()),
        (Some(_), Some(_)) => Err(crate::Error::Token),
    }
}

/// Process failures, told in terms of the GitHub CLI rather than Git.
pub(super) fn process_error(error: crate::Error) -> crate::Error {
    match error {
        crate::Error::PullRequest(pull_request::Error::Process { source, .. })
            if source.kind() == std::io::ErrorKind::NotFound =>
        {
            Error::Missing.into()
        }
        crate::Error::PullRequest(pull_request::Error::Timeout) => Error::Timeout.into(),
        error => Error::Process(Box::new(error)).into(),
    }
}

mod error;
#[cfg(test)]
mod tests;

pub use error::Error;
