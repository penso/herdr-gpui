//! Reaching a saved Azure device. A VM is started if stopped, found by its
//! public IP, and reached through OpenSSH; a sandbox is resumed if suspended
//! and reached through this app's exec stream relay.

use super::{Error, Result, api, gateway::Gateway, http::Client, machine::Machine};
use herdr_client::Transport;
use std::{
    io,
    process::Command,
    sync::atomic::{AtomicBool, Ordering},
};

/// Whether reaching a machine may start it.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Wake {
    Start,
    /// Only `monitor_command` asks for this, on Unix clients.
    #[cfg_attr(not(unix), allow(dead_code))]
    Never,
}

pub(crate) fn command(account: &str, id: &str, stop: &AtomicBool) -> Result<Command> {
    reach(account, id, stop, Wake::Start)
}

/// The command a background monitor reaches a machine with: only one that
/// runs now. A stopped VM or suspended sandbox is left as it is, and the
/// monitor does not count as the activity that holds off auto-stop.
#[cfg(unix)]
pub(crate) fn monitor_command(account: &str, id: &str) -> Result<Command> {
    reach(account, id, &AtomicBool::new(false), Wake::Never)
}

fn reach(account: &str, id: &str, stop: &AtomicBool, wake: Wake) -> Result<Command> {
    if let Some(reason) = crate::cloud::unavailable() {
        return Err(crate::cloud::Error::Unavailable(reason).into());
    }

    let config = crate::config::Config::load().map_err(|error| Error::Storage(Box::new(error)))?;

    let settings = config
        .azure
        .settings()
        .map_err(|error| Error::Storage(Box::new(error)))?
        .ok_or(Error::Account)?;

    let settings = settings.for_device(account)?;

    let cancelled = || stop.load(Ordering::Acquire);
    if cancelled() {
        return Err(Error::Cancelled);
    }

    let id = match Machine::parse(id) {
        Machine::Vm(name) => name,
        Machine::Sandbox(sandbox) if wake == Wake::Never => {
            return super::sandbox::running_command(&settings, sandbox);
        }
        Machine::Sandbox(sandbox) => {
            return super::sandbox::command(&settings, sandbox, &cancelled);
        }
    };

    let client = Client::new(&settings)?;
    if wake == Wake::Never {
        if api::get_vm_power_state(&client, id)? != api::PowerState::Running {
            return Err(Error::NotRunning);
        }
        let ip = api::get_vm_public_ip(&client, id)?
            .filter(|ip| !ip.is_empty())
            .ok_or(Error::NotRunning)?;
        return Gateway::new(&settings.admin_username, &ip)?.build_command();
    }

    // If VM is deallocated or stopped, start it
    if let Ok(state) = api::get_vm_power_state(&client, id)
        && (state == api::PowerState::Deallocated || state == api::PowerState::Stopped)
    {
        let _ = api::start_vm(&client, id);
    }

    let ip = api::wait_running_ip(&client, id, &cancelled)?;
    super::cleanup::record_activity(id);
    let gateway = Gateway::new(&settings.admin_username, &ip)?;
    gateway.build_command()
}

pub(crate) fn connect(
    account: &str,
    id: &str,
    session: &str,
    stop: &AtomicBool,
) -> io::Result<Transport> {
    let result = command(account, id, stop).and_then(|command| {
        herdr_client::connect_command(command, session, stop).map_err(Error::Bridge)
    });
    match result {
        Ok(bridge) => {
            super::cleanup::record_activity(id);
            Ok(bridge.into())
        }
        Err(Error::Bridge(error)) => Err(io::Error::new(error.kind(), error)),
        Err(error) => Err(io::Error::other(error)),
    }
}
