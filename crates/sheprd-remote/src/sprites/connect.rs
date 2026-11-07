//! Reaching a saved sprite: check that it still exists, then run Herdr's
//! stdio bridge through this app's exec relay. A sleeping sprite wakes on the
//! command itself. Runs on the connection worker, and the UI retries it on
//! its own, so it never creates a sprite or installs anything.

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
        .sprites
        .settings()
        .map_err(|error| Error::Storage(Box::new(error)))?
        .ok_or(Error::Account)?;
    if settings.base != account {
        return Err(Error::Account);
    }
    Ok(settings)
}

/// The relay command for `sprite`: this executable run as [`relay::HELPER`].
pub(super) fn relay_command(settings: &Settings, sprite: &str) -> Result<Command> {
    let mut command = crate::cloud::relay::command(relay::HELPER).map_err(Error::Executable)?;
    command.arg(&settings.base).arg(sprite);
    Ok(command)
}

/// The relay command for the sprite `id` in `account`, once the API says it
/// still exists.
pub fn command(account: &str, id: &str, stop: &AtomicBool) -> Result<Command> {
    if let Some(reason) = crate::cloud::unavailable() {
        return Err(crate::cloud::Error::Unavailable(reason).into());
    }
    let settings = settings(account)?;
    if stop.load(Ordering::Acquire) {
        return Err(Error::Cancelled);
    }
    let token = store::token(&settings)?;
    Request {
        settings: &settings,
        token: &token,
    }
    .sprite(id)?;
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
