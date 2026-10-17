//! Creating a sandbox device and the command that reaches one.

use super::{
    super::{
        Error, Result, Settings,
        http::{Audience, Client},
        machine::Machine,
    },
    api, relay,
};
use crate::cloud::{self, CloudProvider, SavedDevice, install};
use std::{
    process::Command,
    time::{Duration, Instant},
};

const SESSION: &str = "default";
/// A new sandbox may run before its exec stream accepts a connection.
const EXEC_WAIT_TIMEOUT: Duration = Duration::from_secs(60);
const EXEC_POLL_INTERVAL: Duration = Duration::from_secs(3);

/// Wake the sandbox if it is suspended and return the command whose standard
/// streams reach it. Runs on a worker: it waits on the network.
pub(crate) fn command(
    settings: &Settings,
    sandbox: &str,
    cancelled: &dyn Fn() -> bool,
) -> Result<Command> {
    let client = Client::for_audience(settings, Audience::Sandboxes)?;
    api::wait_running(&client, settings, sandbox, cancelled)?;
    relay_command(settings, sandbox)
}

/// The relay to a sandbox that runs now; a suspended one is left suspended.
pub(crate) fn running_command(settings: &Settings, sandbox: &str) -> Result<Command> {
    let client = Client::for_audience(settings, Audience::Sandboxes)?;
    if !api::running(&client, settings, sandbox)? {
        return Err(Error::NotRunning);
    }
    relay_command(settings, sandbox)
}

/// Delete the sandbox and wait until it is gone. Runs on a worker.
pub(crate) fn delete(
    settings: &Settings,
    sandbox: &str,
    cancelled: &dyn Fn() -> bool,
) -> Result<()> {
    let client = Client::for_audience(settings, Audience::Sandboxes)?;
    api::delete(&client, settings, sandbox, cancelled)
}

/// Whether the group still has the sandbox, through a sandboxes `client`.
pub(crate) fn exists(client: &Client, settings: &Settings, sandbox: &str) -> Result<bool> {
    api::exists(client, settings, sandbox)
}

/// The size of each sandbox in the group, by ID.
pub(crate) fn sizes(
    settings: &Settings,
) -> Result<std::collections::HashMap<String, super::super::sizes::SandboxSize>> {
    let client = Client::for_audience(settings, Audience::Sandboxes)?;
    api::sizes(&client, settings)
}

/// This executable as the relay, for the remote command a caller appends.
fn relay_command(settings: &Settings, sandbox: &str) -> Result<Command> {
    let executable = std::env::current_exe().map_err(Error::Executable)?;
    let mut command = herdr_client::new_command(executable);
    command
        .arg(relay::HELPER)
        .arg(settings.sandbox_account())
        .arg(sandbox);
    Ok(command)
}

/// Create a sandbox in the configured group, make sure Herdr runs there, and
/// save it as a device.
pub(crate) fn add_device(
    settings: Settings,
    label: String,
    install: bool,
    cancelled: &dyn Fn() -> bool,
    report: &dyn Fn(cloud::Step),
) -> crate::Result<SavedDevice> {
    if cancelled() {
        return Err(Error::Cancelled.into());
    }
    if let Some(reason) = cloud::unavailable() {
        return Err(cloud::Error::Unavailable(reason).into());
    }

    report(cloud::Step::Creating);
    let granted = api::ensure_group(&settings, cancelled)?;
    let client = Client::for_audience(&settings, Audience::Sandboxes)?;
    let sandbox = api::create(&client, &settings, granted, cancelled)?;

    let ready = || -> crate::Result<()> {
        report(cloud::Step::Starting);
        api::wait_running(&client, &settings, &sandbox, cancelled)?;

        report(cloud::Step::CheckingHerdr);
        let command = || relay_command(&settings, &sandbox);
        if !wait_installed(&command, cancelled)? {
            if !install {
                return Err(cloud::Error::Install(format!(
                    "Herdr is not installed on Azure sandbox {sandbox}; enable Install Herdr and retry"
                ))
                .into());
            }
            report(cloud::Step::InstallingHerdr);
            install::install(command()?, &cancelled)?;
            if !install::installed(command()?, &cancelled)? {
                return Err(cloud::Error::Install(
                    "the installer finished but Herdr is not on the expected paths".into(),
                )
                .into());
            }
        }
        Ok(())
    };
    let discard = || {
        super::super::setup::discard_created("sandbox", &sandbox, || {
            api::delete(&client, &settings, &sandbox, &|| false)
        });
    };
    super::super::setup::undo_on_error(ready, discard)?;

    let device = SavedDevice {
        provider: CloudProvider::Azure,
        id: Machine::sandbox_id(&sandbox),
        label,
        account: settings.sandbox_account(),
        machine: sandbox,
        session: SESSION.to_owned(),
        enabled: true,
    };
    cloud::save(device.clone())?;
    Ok(device)
}

fn wait_installed(
    command: &dyn Fn() -> Result<Command>,
    cancelled: &dyn Fn() -> bool,
) -> crate::Result<bool> {
    let deadline = Instant::now() + EXEC_WAIT_TIMEOUT;
    loop {
        match install::installed(command()?, &cancelled) {
            Ok(installed) => return Ok(installed),
            Err(cloud::Error::Cancelled) => return Err(Error::Cancelled.into()),
            Err(error) if Instant::now() >= deadline => return Err(error.into()),
            Err(_) => {}
        }
        let wake = Instant::now() + EXEC_POLL_INTERVAL;
        while Instant::now() < wake {
            if cancelled() {
                return Err(Error::Cancelled.into());
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }
}
