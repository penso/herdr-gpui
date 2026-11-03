//! Blocking work behind Boat's Settings tab: record a created sandbox before
//! setup, wait, check/install Herdr, and summarize the configured account.

use super::{
    Result, Settings,
    api::{self, Readiness},
    gateway,
    http::Request,
    pending::{self, Journal},
    store,
};
use crate::cloud::{self, CloudProvider, SavedDevice, install};
use crate::credentials::Store;
use secrecy::SecretString;

const SESSION: &str = "default";
const NAME_LIMIT: usize = 40;

pub struct AddRequest {
    pub settings: Settings,
    pub target: Target,
    pub install: bool,
}

pub enum Target {
    Create(String),
    Saved(SavedDevice),
}

pub fn random_name() -> String {
    cloud::names::random("herdr", NAME_LIMIT)
}

fn step(readiness: &Readiness) -> cloud::Step {
    match readiness {
        Readiness::Pending(state) => cloud::Step::Building(state.clone()),
        _ => cloud::Step::Starting,
    }
}

pub fn add_device(
    request: AddRequest,
    cancelled: &dyn Fn() -> bool,
    report: &dyn Fn(cloud::Step),
) -> crate::Result<SavedDevice> {
    if cancelled() {
        return Err(super::Error::Cancelled.into());
    }
    let AddRequest {
        settings,
        target,
        install,
    } = request;
    if let Target::Saved(device) = &target
        && (device.provider != CloudProvider::Boat || device.account != settings.account())
    {
        return Err(super::Error::Account.into());
    }
    if let Some(reason) = cloud::unavailable() {
        return Err(cloud::Error::Unavailable(reason).into());
    }
    gateway::check_ssh()?;
    let key = store::key(&settings)?;
    let request = Request {
        settings: &settings,
        key: &key,
    };
    finish_setup(
        &request,
        &pending::directory()?,
        target,
        cancelled,
        report,
        |device| Ok(cloud::save(device)?),
        |sandbox| ensure_herdr(&request, sandbox, install, cancelled, report),
    )
}

fn save_device(
    device: &SavedDevice,
    save: &mut impl FnMut(SavedDevice) -> crate::Result<()>,
) -> Result<()> {
    save(device.clone()).map_err(|source| super::Error::SaveDevice {
        id: device.id.clone(),
        source: Box::new(source),
    })
}

fn new_device(settings: &Settings, sandbox: &api::Sandbox, label: String) -> SavedDevice {
    SavedDevice {
        provider: CloudProvider::Boat,
        id: sandbox.id.clone(),
        label,
        account: settings.account(),
        machine: sandbox.name.clone(),
        session: SESSION.to_owned(),
        enabled: true,
    }
}

fn finish_setup(
    request: &Request,
    directory: &std::path::Path,
    target: Target,
    cancelled: &dyn Fn() -> bool,
    report: &dyn Fn(cloud::Step),
    mut save: impl FnMut(SavedDevice) -> crate::Result<()>,
    prepare: impl FnOnce(&api::Sandbox) -> crate::Result<()>,
) -> crate::Result<SavedDevice> {
    let mut device = match target {
        Target::Create(name) => {
            report(cloud::Step::Creating);
            // Cancellation must not lose a billable resource whose ID we know.
            // Save it even if cancellation arrived while create was in flight.
            let journal = Journal::open(directory, request.settings)?;
            let creation = journal.begin(request.settings, &name)?;
            let created = request
                .create(&creation, cancelled, |created| {
                    save_device(
                        &new_device(request.settings, created, creation.name.clone()),
                        &mut save,
                    )
                })
                .map_err(|error| match error {
                    error @ super::Error::SaveDevice { .. } => error,
                    error => super::Error::PendingCreation(Box::new(error)),
                })?;
            journal.clear()?;
            new_device(request.settings, &created, creation.name)
        }
        Target::Saved(device) => device,
    };
    let setup_error = |source| super::Error::Setup {
        id: device.id.clone(),
        source: Box::new(source),
    };
    let sandbox = api::wait_ready(request, &device.id, cancelled, |readiness| {
        report(step(readiness));
    })
    .map_err(crate::Error::from)
    .map_err(&setup_error)?;
    prepare(&sandbox).map_err(setup_error)?;
    device.machine = sandbox.name;
    save_device(&device, &mut save)?;
    Ok(device)
}

fn ensure_herdr(
    request: &Request,
    sandbox: &api::Sandbox,
    install: bool,
    cancelled: &dyn Fn() -> bool,
    report: &dyn Fn(cloud::Step),
) -> crate::Result<()> {
    report(cloud::Step::CheckingHerdr);
    if cancelled() {
        return Err(super::Error::Cancelled.into());
    }
    request.extend(&sandbox.id)?;
    if cancelled() {
        return Err(super::Error::Cancelled.into());
    }
    let installed = gateway::prepare(request, &sandbox.id)?
        .with_command(|command| install::installed(command, &cancelled))?;
    if !installed {
        if !install {
            return Err(cloud::Error::Install(install::Failure::NotInstalled {
                machine: Some(sandbox.id.clone()),
                retry: install::Retry::FinishSetup,
            })
            .into());
        }
        report(cloud::Step::InstallingHerdr);
        gateway::prepare(request, &sandbox.id)?
            .with_command(|command| install::install(command, &cancelled))?;
        if !gateway::prepare(request, &sandbox.id)?
            .with_command(|command| install::installed(command, &cancelled))?
        {
            return Err(cloud::Error::Install(install::Failure::Missing).into());
        }
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    Missing,
    Saved,
    Environment,
}

#[derive(Debug)]
pub struct Overview {
    pub key: Result<Key>,
    pub sandboxes: Option<Result<api::SandboxCount>>,
    pub devices: Vec<SavedDevice>,
    pub pending: Result<Option<String>>,
}

pub fn overview(settings: &Settings) -> Result<Overview> {
    let account = settings.account();
    let devices = cloud::load()?
        .into_iter()
        .filter(|saved| saved.provider == CloudProvider::Boat && saved.account == account)
        .collect();
    let key = if std::env::var_os(store::VARIABLE).is_some() {
        Ok(Key::Environment)
    } else {
        store::saved(settings.store).map(|saved| {
            if saved.is_some() {
                Key::Saved
            } else {
                Key::Missing
            }
        })
    };
    let mut overview = account_overview(devices, key, || {
        let key = store::key(settings)?;
        Request {
            settings,
            key: &key,
        }
        .sandbox_count()
    });
    overview.pending = pending::overview(settings);
    Ok(overview)
}

fn account_overview(
    devices: Vec<SavedDevice>,
    key: Result<Key>,
    list: impl FnOnce() -> Result<api::SandboxCount>,
) -> Overview {
    let sandboxes = matches!(key, Ok(Key::Saved | Key::Environment)).then(list);
    Overview {
        key,
        sandboxes,
        devices,
        pending: Ok(None),
    }
}

pub fn save_key(store: Store, key: Option<&SecretString>) -> Result<()> {
    store::save(store, key)
}

pub fn forget_device(id: &str) -> Result<()> {
    Ok(cloud::remove(CloudProvider::Boat, id)?)
}

pub fn forget_pending(settings: &Settings) -> Result<()> {
    pending::forget(settings)
}

#[cfg(test)]
mod tests;
