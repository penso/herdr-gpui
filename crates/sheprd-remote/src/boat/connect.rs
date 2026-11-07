//! Reaching a saved Boat sandbox: resume it if stopped, authorize the app's
//! SSH key, pin Boat's authenticated host key, and run Herdr's stdio bridge.

use super::{Error, Result, Settings, api, gateway, http::Request, store};
use sheprd_client::Transport;
use std::{
    io,
    sync::atomic::{AtomicBool, Ordering},
};

fn sandbox_gateway(settings: &Settings, id: &str, stop: &AtomicBool) -> Result<gateway::Gateway> {
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
                category = "boat_connect",
                ?readiness,
                "Waiting for Boat sandbox"
            );
        },
    )?;
    gateway::prepare(&request, &sandbox.id)
}

/// Prepare the authenticated command for a connection or background script.
pub fn command(account: &str, id: &str, stop: &AtomicBool) -> Result<gateway::Gateway> {
    if let Some(reason) = crate::cloud::unavailable() {
        return Err(crate::cloud::Error::Unavailable(reason).into());
    }
    crate::config::load()
        .map_err(|error| Error::Storage(Box::new(error)))
        .and_then(|config| {
            config
                .boat
                .settings()
                .map_err(|error| Error::Storage(Box::new(error)))
        })
        .and_then(|settings| {
            let settings = settings.ok_or(Error::Account)?;
            if settings.account() != account {
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

#[cfg(all(test, windows))]
mod tests;
