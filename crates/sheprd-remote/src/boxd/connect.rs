//! Reaching a saved boxd machine: start, resume, or wake it if it sleeps,
//! then run Herdr's stdio bridge through this app's exec relay. Runs on the
//! connection worker, and the UI retries it on its own, so it never creates
//! a machine or installs anything.

use super::{Error, Result, Settings, http::Request, relay, store};
use sheprd_client::Transport;
use std::{
    io,
    process::Command,
    sync::atomic::{AtomicBool, Ordering},
};

/// The configured account's settings, which must be the one `account` names.
/// Configuration is read here, on the worker, so an account changed in the
/// config file applies on retry.
pub fn settings(account: &str) -> Result<Settings> {
    let config = crate::config::load().map_err(|error| Error::Storage(Box::new(error)))?;
    let settings = config
        .boxd
        .settings()
        .map_err(|error| Error::Storage(Box::new(error)))?
        .ok_or(Error::Account)?;
    if settings.base != account {
        return Err(Error::Account);
    }
    Ok(settings)
}

/// The relay command for `machine`: this executable run as [`relay::HELPER`].
pub(super) fn relay_command(settings: &Settings, machine: &str) -> Result<Command> {
    let executable = std::env::current_exe().map_err(Error::Executable)?;
    let mut command = sheprd_client::new_command(executable);
    command.arg(relay::HELPER).arg(&settings.base).arg(machine);
    Ok(command)
}

/// The relay command for the machine `id` in `account`, once it runs.
pub fn command(account: &str, id: &str, stop: &AtomicBool) -> Result<Command> {
    if let Some(reason) = crate::cloud::unavailable() {
        return Err(crate::cloud::Error::Unavailable(reason).into());
    }
    let settings = settings(account)?;
    let key = store::key(&settings)?;
    Request {
        settings: &settings,
        key: &key,
    }
    .wait_ready(
        id,
        || stop.load(Ordering::Acquire),
        |readiness| {
            tracing::info!(
                category = "boxd_connect",
                ?readiness,
                "Waiting for boxd machine"
            );
        },
    )?;
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
