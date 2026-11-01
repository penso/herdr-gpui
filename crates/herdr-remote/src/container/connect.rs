//! Reaching a saved container: start it if it stopped, then run Herdr's stdio
//! bridge through `exec`. Runs on the connection worker, and the UI retries
//! it on its own, so it never creates a container or installs anything.

use super::{Engine, Error, Result, machine};
use crate::cloud::Value;
use herdr_client::Transport;
use std::{
    io,
    process::Command,
    sync::atomic::{AtomicBool, Ordering},
};

/// The engine a saved device was made with, from its account.
fn engine(account: &str) -> Result<Engine> {
    Engine::from_key(account).ok_or(Error::Field(Value::ContainerEngine))
}

/// The `exec` command for the container `id`, started first if it stopped.
pub fn command(account: &str, id: &str, stop: &AtomicBool) -> Result<Command> {
    let engine = engine(account)?;
    super::lifecycle::start(engine, id, || {
        machine::ensure_running(engine, id, &|| stop.load(Ordering::Acquire))
    })?;
    machine::exec(engine, id)
}

/// Reach the container `id` for `cloud::connect`.
pub fn connect(account: &str, id: &str, session: &str, stop: &AtomicBool) -> io::Result<Transport> {
    let result = command(account, id, stop).and_then(|command| {
        herdr_client::connect_command(command, session, stop).map_err(Error::Bridge)
    });
    match result {
        Ok(bridge) => Ok(bridge.into()),
        Err(Error::Bridge(error)) => Err(io::Error::new(error.kind(), error)),
        Err(error) => Err(io::Error::other(error)),
    }
}
