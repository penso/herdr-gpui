//! Reaching a saved sandbox: resume it if it was paused, then run Herdr's
//! stdio bridge through this app's exec relay. Runs on the connection worker,
//! and the UI retries it on its own, so it never creates a sandbox or
//! installs anything.

use super::{Error, Result, Settings, api::Api, relay, store};
use sheprd_client::Transport;
use std::{
    io,
    process::Command,
    sync::atomic::{AtomicBool, Ordering},
};

/// The configured server's settings, which must be the one `account` names.
/// Configuration is read here, on the worker, so a server changed in the
/// config file applies on retry.
pub fn settings(account: &str) -> Result<Settings> {
    let config = crate::config::load().map_err(|error| Error::Storage(Box::new(error)))?;
    let settings = config
        .opensandbox
        .settings()
        .map_err(|error| Error::Storage(Box::new(error)))?
        .ok_or(Error::Account)?;
    if settings.base != account {
        return Err(Error::Account);
    }
    Ok(settings)
}

/// The relay command for `sandbox`: this executable run as [`relay::HELPER`].
pub(super) fn relay_command(settings: &Settings, sandbox: &str) -> Result<Command> {
    let executable = std::env::current_exe().map_err(Error::Executable)?;
    let mut command = sheprd_client::new_command(executable);
    command.arg(relay::HELPER).arg(&settings.base).arg(sandbox);
    Ok(command)
}

/// The relay command for the sandbox `id` on `account`, once it runs.
pub fn command(account: &str, id: &str, stop: &AtomicBool) -> Result<Command> {
    if let Some(reason) = crate::cloud::unavailable() {
        return Err(crate::cloud::Error::Unavailable(reason).into());
    }
    let settings = settings(account)?;
    let key = store::key(&settings)?;
    Api {
        settings: &settings,
        key: &key,
    }
    .wait_running(id, &|| stop.load(Ordering::Acquire))?;
    relay_command(&settings, id)
}

pub fn connect(account: &str, id: &str, session: &str, stop: &AtomicBool) -> io::Result<Transport> {
    let result = command(account, id, stop).and_then(|command| {
        sheprd_client::connect_command(command, session, stop).map_err(Error::Bridge)
    });
    match result {
        Ok(bridge) => Ok(bridge.into()),
        Err(Error::Bridge(error)) => Err(io::Error::new(error.kind(), error)),
        Err(error) => Err(io::Error::other(error)),
    }
}
