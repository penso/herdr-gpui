//! Reaching a saved Daytona sandbox: start it if it stopped, issue an SSH
//! token for it, and run Herdr's stdio bridge through the gateway. Runs on the
//! connection worker; every wait observes `stop`.

use super::{
    Error, Result, Settings,
    api::{self, Sandbox},
    gateway::{self, Gateway},
    http::Request,
    store,
};
use sheprd_client::Transport;
use std::{
    io,
    sync::atomic::{AtomicBool, Ordering},
};

/// An `ssh` command through the gateway for the started sandbox `sandbox`.
pub fn gateway(request: &Request, sandbox: &Sandbox) -> Result<Gateway> {
    Gateway::new(&request.ssh_access(&sandbox.id)?, &gateway::known_hosts()?)
}

fn sandbox_gateway(settings: &Settings, id: &str, stop: &AtomicBool) -> Result<Gateway> {
    let key = store::key(settings)?;
    let request = Request {
        settings,
        key: &key,
    };
    let sandbox = api::wait_ready(
        &request,
        id,
        || stop.load(Ordering::Acquire),
        |readiness| {
            tracing::info!(
                category = "daytona_connect",
                ?readiness,
                "Waiting for Daytona sandbox"
            );
        },
    )?;
    gateway(&request, &sandbox)
}

/// Reach the Daytona sandbox `id` in `account` for `cloud::connect`.
/// Configuration is read here, on the worker, so an account changed in the
/// config file applies on retry.
pub fn command(account: &str, id: &str, stop: &AtomicBool) -> Result<Gateway> {
    crate::config::load()
        .map_err(|error| Error::Storage(Box::new(error)))
        .and_then(|config| {
            config
                .daytona
                .settings()
                .map_err(|error| Error::Storage(Box::new(error)))
        })
        .and_then(|settings| {
            let settings = settings.ok_or(Error::Account)?;
            if settings.base != account {
                return Err(Error::Account);
            }
            sandbox_gateway(&settings, id, stop)
        })
}

pub fn connect(account: &str, id: &str, session: &str, stop: &AtomicBool) -> io::Result<Transport> {
    let result = command(account, id, stop).and_then(|gateway| {
        gateway.with_command(|command| {
            sheprd_client::connect_command(command, session, stop).map_err(Error::Bridge)
        })
    });
    match result {
        Ok(bridge) => Ok(bridge.into()),
        Err(Error::Bridge(error)) => Err(io::Error::new(error.kind(), error)),
        Err(error) => Err(io::Error::other(error)),
    }
}
