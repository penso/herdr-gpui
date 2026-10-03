//! Reaching a saved Coder workspace: make sure it is running, then run
//! Herdr's stdio bridge through `coder ssh`. Coder's tunnel authenticates the
//! connection, so there is no SSH config edit or host-key trust decision here.
//! Runs on the connection worker; every wait observes `stop`.

use super::{Error, Result, Settings, api, store};
use herdr_client::{ConnectTarget, Transport};
use secrecy::{ExposeSecret, SecretString};
use std::{
    io,
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicBool, Ordering},
};

/// Places a `coder` binary is commonly installed. A GUI launched from the
/// Finder does not inherit a login shell's PATH, so PATH alone is not enough.
const SEARCH: &[&str] = &["/opt/homebrew/bin", "/usr/local/bin", "/usr/bin"];

/// The `coder` executable: the configured path, else PATH, else the usual roots.
pub(crate) fn cli(settings: &Settings) -> Result<PathBuf> {
    if let Some(path) = &settings.cli {
        return Ok(path.clone());
    }
    let home = std::env::var_os("HOME").map(PathBuf::from);
    std::env::var_os("PATH")
        .into_iter()
        .flat_map(|path| std::env::split_paths(&path).collect::<Vec<_>>())
        .chain(SEARCH.iter().map(PathBuf::from))
        .chain(
            home.iter()
                .flat_map(|home| [home.join(".local/bin"), home.join("bin")]),
        )
        .map(|dir| dir.join("coder"))
        .find(|candidate| executable(candidate))
        .ok_or(Error::Cli)
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

/// `coder ssh` for one workspace agent, authenticated only through this
/// child's environment. The remote command is appended by the caller.
pub(crate) fn ssh_command(
    cli: &Path,
    settings: &Settings,
    token: &SecretString,
    workspace: &str,
    agent: &str,
) -> Result<Command> {
    if !super::names::valid(workspace)
        || agent.is_empty()
        || agent.len() > 64
        || !agent
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        return Err(Error::Field("Coder workspace"));
    }
    let mut command = Command::new(cli);
    command
        .env("CODER_URL", &settings.base)
        .env("CODER_SESSION_TOKEN", token.expose_secret())
        // Readiness was checked through the API; do not block on startup logs.
        .args(["ssh", "--wait=no", "--", &format!("{workspace}.{agent}")]);
    Ok(command)
}

/// The workspace ID for `name`, owned by the signed-in user.
fn find(
    settings: &Settings,
    tokens: &impl Fn(bool) -> Result<SecretString>,
    name: &str,
) -> Result<String> {
    store::with_token(tokens, |token| {
        api::Client::new(settings, token)
            .workspaces()
            .map(|workspaces| {
                workspaces
                    .into_iter()
                    .find(|w| w.name == name)
                    .map(|w| w.id)
            })
    })?
    .ok_or(Error::Deleted)
}

fn connect_workspace(
    settings: &Settings,
    workspace: &str,
    session: &str,
    stop: &AtomicBool,
) -> Result<herdr_client::Bridge> {
    let tokens = |rejected| store::current_token(settings, rejected);
    let cancelled = || stop.load(Ordering::Acquire);
    let id = find(settings, &tokens, workspace)?;
    let (_, agent) = api::wait_ready(settings, &tokens, &id, cancelled, |progress| {
        tracing::info!(
            category = "coder_connect",
            ?progress,
            "Waiting for Coder workspace"
        );
    })?;
    let command = ssh_command(
        &cli(settings)?,
        settings,
        &tokens(false)?,
        workspace,
        &agent,
    )?;
    herdr_client::connect_command(command, session, stop).map_err(Error::Bridge)
}

/// The connector for `ConnectTarget::Coder`. Configuration is read here, on
/// the worker, so a deployment changed in the config file applies on retry.
pub(crate) fn connect(target: &ConnectTarget, stop: &AtomicBool) -> io::Result<Transport> {
    let ConnectTarget::Coder {
        deployment,
        workspace,
        session,
    } = target
    else {
        return Err(io::Error::other(Error::Field("Coder target")));
    };
    let result = crate::config::Config::load()
        .map_err(|error| Error::Storage(Box::new(error)))
        .and_then(|config| {
            config
                .coder
                .settings()
                .map_err(|error| Error::Storage(Box::new(error)))
        })
        .and_then(|settings| {
            let settings = settings.ok_or(Error::Missing("url"))?;
            if &settings.base != deployment {
                return Err(Error::Deployment);
            }
            connect_workspace(&settings, workspace, session, stop)
        });
    match result {
        Ok(bridge) => Ok(bridge.into()),
        Err(Error::Bridge(error)) => Err(io::Error::new(error.kind(), error)),
        Err(error) => Err(io::Error::other(error)),
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use crate::coder::tests::settings;

    #[test]
    fn coder_ssh_carries_the_token_only_in_the_child_environment() {
        let command = ssh_command(
            Path::new("/usr/local/bin/coder"),
            &settings(),
            &"token-fixture".into(),
            "herdr-box",
            "main",
        )
        .unwrap();
        let args: Vec<_> = command.get_args().map(|a| a.to_str().unwrap()).collect();
        assert_eq!(args, ["ssh", "--wait=no", "--", "herdr-box.main"]);
        assert!(!args.iter().any(|arg| arg.contains("token-fixture")));
        let envs: std::collections::HashMap<_, _> = command
            .get_envs()
            .map(|(k, v)| (k.to_str().unwrap(), v.unwrap().to_str().unwrap()))
            .collect();
        assert_eq!(envs["CODER_SESSION_TOKEN"], "token-fixture");
        assert_eq!(envs["CODER_URL"], "https://coder.example.com");
        for (workspace, agent) in [("-oops", "main"), ("herdr-box", "a.b"), ("herdr-box", "")] {
            assert!(
                ssh_command(
                    Path::new("coder"),
                    &settings(),
                    &"t".into(),
                    workspace,
                    agent
                )
                .is_err()
            );
        }
    }

    #[test]
    fn a_configured_cli_path_wins_over_discovery() {
        let mut settings = settings();
        settings.cli = Some("/custom/coder".into());
        assert_eq!(cli(&settings).unwrap(), PathBuf::from("/custom/coder"));
    }
}
