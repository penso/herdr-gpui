//! Helper for querying the local Azure CLI (`az`) when present. This lets users
//! and agents run without configuring a Service Principal or client secret.

use super::{Error, Result};
use std::{
    path::{Path, PathBuf},
    process::Stdio,
};

const SEARCH: &[&str] = &["/opt/homebrew/bin", "/usr/local/bin", "/usr/bin", "/bin"];

/// The CLI's file names: Windows installs it as a batch script, `az.cmd`.
#[cfg(windows)]
const NAMES: &[&str] = &["az.cmd", "az.exe", "az"];
#[cfg(not(windows))]
const NAMES: &[&str] = &["az"];

/// Find the `az` executable on PATH or in standard directories.
pub(crate) fn find_az() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("AZURE_CLI_PATH").map(PathBuf::from)
        && executable(&path)
    {
        return Some(path);
    }
    let path = std::env::var_os("PATH").unwrap_or_default();
    find_in(std::env::split_paths(&path).chain(SEARCH.iter().map(PathBuf::from)))
}

/// The first CLI executable in `dirs`, trying each of [`NAMES`] in turn.
fn find_in(dirs: impl IntoIterator<Item = PathBuf>) -> Option<PathBuf> {
    dirs.into_iter()
        .flat_map(|dir| NAMES.iter().map(move |name| dir.join(name)))
        .find(|candidate| executable(candidate))
}

#[cfg(unix)]
fn executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    path.metadata()
        .is_ok_and(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
}

#[cfg(not(unix))]
fn executable(path: &Path) -> bool {
    path.is_file()
}

pub(crate) fn is_az_available() -> bool {
    find_az().is_some()
}

fn run_az(args: &[&str]) -> Result<String> {
    let az_bin = find_az().ok_or(Error::AzMissing)?;
    let mut command = herdr_client::new_command(az_bin);
    command.args(args);
    command.stdin(Stdio::null());
    crate::login_env::apply(&mut command);

    let output = command.output().map_err(Error::Process)?;
    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        return Err(Error::AzFailed(err.trim().to_owned()));
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(stdout.trim().to_owned())
}

/// Retrieve an access token for `audience` from `az`, issued by the tenant
/// that owns `subscription` rather than whichever account `az` has active.
pub(crate) fn access_token(audience: super::http::Audience, subscription: &str) -> Result<String> {
    run_az(&[
        "account",
        "get-access-token",
        "--resource",
        audience.resource(),
        "--subscription",
        subscription,
        "--query",
        "accessToken",
        "-o",
        "tsv",
    ])
}

/// Retrieve the currently active subscription ID from `az`.
pub(crate) fn default_subscription() -> Result<String> {
    run_az(&["account", "show", "--query", "id", "-o", "tsv"])
}

/// Retrieve the currently active tenant ID from `az`.
pub(crate) fn default_tenant() -> Result<String> {
    run_az(&["account", "show", "--query", "tenantId", "-o", "tsv"])
}

#[cfg(test)]
mod tests;
