//! A checkout's own GitLab token: the `GITLAB_TOKEN` its `.envrc` sets, as
//! direnv loads it there. direnv runs only an `.envrc` the user allowed with
//! `direnv allow`, so this runs nothing the user's own shell would not.

use super::{CLI_TIMEOUT, TOKEN_VARIABLES, environment_token};
use secrecy::{ExposeSecret, SecretString};
use std::{
    ffi::OsString,
    path::{Path, PathBuf},
    sync::{Mutex, PoisonError},
    time::{Duration, Instant, SystemTime},
};

/// The whole environment `env` prints; the rest of a large one is dropped.
const OUTPUT_LIMIT: usize = 256 * 1024;
/// How long what an `.envrc` loads is reused while the file is unchanged.
/// Loading one may evaluate a Nix shell, too slow to repeat on every lookup.
const REUSE: Duration = Duration::from_secs(5 * 60);
/// The most checkouts whose variables are kept.
const CHECKOUTS: usize = 32;

/// The GitLab variables each checkout's `.envrc` loaded, newest last.
static LOADED: Mutex<Vec<Loaded>> = Mutex::new(Vec::new());

struct Loaded {
    checkout: PathBuf,
    /// Each `.envrc` direnv would load, and when it last changed.
    files: Vec<(PathBuf, Option<SystemTime>)>,
    at: Instant,
    variables: Vec<(String, SecretString)>,
}

/// Every `.envrc` direnv may load for `checkout`, in it or a folder above
/// it, with when each last changed; empty when there is none.
fn envrc_files(checkout: &Path) -> Vec<(PathBuf, Option<SystemTime>)> {
    checkout
        .ancestors()
        .map(|folder| folder.join(".envrc"))
        .filter(|file| file.is_file())
        .map(|file| {
            let modified = file.metadata().and_then(|meta| meta.modified()).ok();
            (file, modified)
        })
        .collect()
}

/// The token `checkout`'s `.envrc` sets for instance `host`, if direnv is
/// installed and the file is allowed. Blocks on a subprocess, which
/// `cancelled` stops: call it from a worker.
pub fn token(checkout: &Path, host: &str, cancelled: &impl Fn() -> bool) -> Option<SecretString> {
    let files = envrc_files(checkout);
    if files.is_empty() {
        return None;
    }
    let variables = match reused(checkout, &files) {
        Some(variables) => variables,
        None => {
            let variables = load(checkout, cancelled)?;
            keep(checkout, files, &variables);
            variables
        }
    };
    token_in(&variables, host)
}

/// What `checkout`'s `.envrc` loaded, while every file is unchanged.
fn reused(
    checkout: &Path,
    files: &[(PathBuf, Option<SystemTime>)],
) -> Option<Vec<(String, SecretString)>> {
    let loaded = LOADED.lock().unwrap_or_else(PoisonError::into_inner);
    loaded
        .iter()
        .find(|loaded| {
            loaded.checkout == checkout && loaded.files == files && loaded.at.elapsed() < REUSE
        })
        .map(|loaded| loaded.variables.clone())
}

fn keep(
    checkout: &Path,
    files: Vec<(PathBuf, Option<SystemTime>)>,
    variables: &[(String, SecretString)],
) {
    let mut loaded = LOADED.lock().unwrap_or_else(PoisonError::into_inner);
    loaded.retain(|loaded| loaded.checkout != checkout);
    if loaded.len() >= CHECKOUTS {
        loaded.remove(0);
    }
    loaded.push(Loaded {
        checkout: checkout.to_owned(),
        files,
        at: Instant::now(),
        variables: variables.to_vec(),
    });
}

/// The GitLab variables direnv loads in `checkout`, or `None` when it
/// cannot: not installed, the file not allowed, or `cancelled`.
fn load(checkout: &Path, cancelled: &impl Fn() -> bool) -> Option<Vec<(String, SecretString)>> {
    let mut command = herdr_client::new_command("direnv");
    // An app opened from a desktop launcher has a minimal PATH.
    herdr_client::login_env::apply(&mut command);
    // Only a token the .envrc sets counts as the checkout's, not one this
    // process or the login shell already had. GITLAB_HOST stays, so a token
    // the .envrc sets applies where glab in that folder would send it.
    for name in TOKEN_VARIABLES {
        command.env_remove(name);
    }
    command
        .env("DIRENV_LOG_FORMAT", "")
        .arg("exec")
        .arg(checkout)
        .arg("env");
    let result = herdr_github::pull_request::run_bytes(
        &mut command,
        Instant::now() + CLI_TIMEOUT,
        cancelled,
        OUTPUT_LIMIT,
    );
    match result {
        Ok((true, output)) => Some(variables(&output)),
        Ok((false, _)) => None,
        Err(error) => {
            tracing::debug!(
                category = "orchestrator_gitlab",
                error = %error,
                "direnv could not be asked for a checkout's GitLab token"
            );
            None
        }
    }
}

/// The GitLab variables in `env`'s output: `GITLAB_HOST` and the tokens.
fn variables(output: &[u8]) -> Vec<(String, SecretString)> {
    let text = String::from_utf8_lossy(output);
    text.lines()
        .filter_map(|line| line.split_once('='))
        .filter(|(name, _)| *name == "GITLAB_HOST" || TOKEN_VARIABLES.contains(name))
        .map(|(name, value)| (name.to_owned(), SecretString::from(value)))
        .collect()
}

fn token_in(variables: &[(String, SecretString)], host: &str) -> Option<SecretString> {
    environment_token(host, |name| {
        variables
            .iter()
            .find(|(variable, _)| variable == name)
            .map(|(_, value)| OsString::from(value.expose_secret()))
    })
}

/// The token for `host` in the output of `env` run where the `.envrc` was
/// loaded, which carries no token but the ones it sets.
pub fn from_env(output: &[u8], host: &str) -> Option<SecretString> {
    token_in(&variables(output), host)
}
