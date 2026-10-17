//! Background work behind Azure's Settings tab: deploy a VM, wait for IP,
//! check/install Herdr, and summarize the configured account.

use super::{
    Result, Settings, api,
    gateway::{self, Gateway},
    http::Client,
    store,
};
use crate::cloud::{self, CloudProvider, SavedDevice, install};
use crate::github::Store;
use secrecy::SecretString;

use std::time::{Duration, Instant};

const SESSION: &str = "default";
const NAME_LIMIT: usize = 40;
const SSH_WAIT_TIMEOUT: Duration = Duration::from_secs(120);
const SSH_POLL_INTERVAL: Duration = Duration::from_secs(3);

pub(crate) struct AddRequest {
    pub(crate) settings: Settings,
    pub(crate) target: Target,
    pub(crate) install: bool,
}

pub(crate) enum Target {
    Create(String),
    #[allow(dead_code)]
    Saved(SavedDevice),
}

pub(crate) fn random_name() -> String {
    cloud::names::random("herdr", NAME_LIMIT)
}

pub(crate) fn add_device(
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

    let settings = match &target {
        Target::Saved(device) if device.provider != CloudProvider::Azure => {
            return Err(super::Error::Account.into());
        }
        Target::Saved(device) => settings.for_device(&device.account)?,
        Target::Create(_) => settings,
    };
    if let Some(reason) = cloud::unavailable() {
        return Err(cloud::Error::Unavailable(reason).into());
    }

    let client = Client::new(&settings)?;
    let (_, public_key) = gateway::persistent_key()?;

    let (vm_name, label) = match &target {
        Target::Create(name) => (name.clone(), name.clone()),
        Target::Saved(device) => (device.id.clone(), device.label.clone()),
    };

    let device = SavedDevice {
        provider: CloudProvider::Azure,
        id: vm_name.clone(),
        label,
        account: settings.vm_account(),
        machine: vm_name.clone(),
        session: SESSION.to_owned(),
        enabled: true,
    };
    let ready = || -> crate::Result<()> {
        if matches!(&target, Target::Create(_)) {
            report(cloud::Step::Creating);
            // Settings shows which priority it got, on the device's row.
            let priority = api::deploy_vm(&client, &vm_name, &public_key)?;
            tracing::info!(
                category = "azure_vm",
                vm_name,
                ?priority,
                "Azure VM deployed"
            );
        }
        report(cloud::Step::Starting);
        let ip = api::wait_running_ip(&client, &vm_name, cancelled)?;
        let gateway = Gateway::new(&settings.admin_username, &ip)?;
        ensure_herdr(&gateway, &vm_name, install, cancelled, report)
    };
    let discard = || {
        if matches!(&target, Target::Create(_)) {
            discard_created("VM", &vm_name, || {
                api::delete_vm(&client, &vm_name, &|| false, || {})
            });
        }
    };
    undo_on_error(ready, discard)?;

    cloud::save(device.clone())?;

    Ok(device)
}

/// Run `attempt`, and `undo` when it fails, returning the attempt's error.
/// A create that fails or is cancelled after Azure made its machine would
/// otherwise leave it billing with no saved device to find it by.
pub(super) fn undo_on_error<T, E>(
    attempt: impl FnOnce() -> std::result::Result<T, E>,
    undo: impl FnOnce(),
) -> std::result::Result<T, E> {
    attempt().inspect_err(|_| undo())
}

/// Delete a machine this create made, after the create failed. The create's
/// own error is what the user sees, so a failed delete is only logged.
pub(super) fn discard_created<E: std::fmt::Display>(
    kind: &str,
    name: &str,
    delete: impl FnOnce() -> std::result::Result<(), E>,
) {
    tracing::info!(
        category = "azure_vm",
        kind,
        name,
        "Deleting a failed create"
    );
    if let Err(error) = delete() {
        tracing::warn!(
            category = "azure_vm",
            kind,
            name,
            %error,
            "Could not delete a failed create; delete it in the Azure portal"
        );
    }
}

fn ensure_herdr(
    gateway: &Gateway,
    vm_name: &str,
    install: bool,
    cancelled: &dyn Fn() -> bool,
    report: &dyn Fn(cloud::Step),
) -> crate::Result<()> {
    report(cloud::Step::CheckingHerdr);
    if cancelled() {
        return Err(super::Error::Cancelled.into());
    }

    let installed = wait_installed(gateway, cancelled)?;
    if !installed {
        if !install {
            return Err(cloud::Error::Install(format!(
                "Herdr is not installed on Azure VM {}; enable Install Herdr and retry",
                vm_name
            ))
            .into());
        }
        report(cloud::Step::InstallingHerdr);
        gateway.with_command(|command| install::install(command, &cancelled))??;
        if !gateway.with_command(|command| install::installed(command, &cancelled))?? {
            return Err(cloud::Error::Install(
                "the installer finished but Herdr is not on the expected paths".into(),
            )
            .into());
        }
    }
    Ok(())
}

fn wait_installed(gateway: &Gateway, cancelled: &dyn Fn() -> bool) -> crate::Result<bool> {
    let deadline = Instant::now() + SSH_WAIT_TIMEOUT;
    loop {
        if cancelled() {
            return Err(super::Error::Cancelled.into());
        }
        let result = gateway.with_command(|command| install::installed(command, &cancelled))?;
        match result {
            Ok(installed) => return Ok(installed),
            Err(cloud::Error::Cancelled) => return Err(super::Error::Cancelled.into()),
            Err(error) => {
                if Instant::now() >= deadline {
                    return Err(error.into());
                }
                let wake = Instant::now() + SSH_POLL_INTERVAL;
                while Instant::now() < wake {
                    if cancelled() {
                        return Err(super::Error::Cancelled.into());
                    }
                    std::thread::sleep(Duration::from_millis(100));
                }
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CredentialStatus {
    Missing,
    SavedSecret,
    EnvironmentSecret,
    AzureCli,
}

#[derive(Debug)]
pub(crate) struct Overview {
    pub(crate) credential: Result<CredentialStatus>,
    pub(crate) devices: Vec<SavedDevice>,
}

pub(crate) fn overview(settings: &Settings) -> Result<Overview> {
    let devices = cloud::load()?
        .into_iter()
        .filter(|saved| saved.provider == CloudProvider::Azure && settings.owns(&saved.account))
        .collect();

    let credential = if std::env::var_os(store::VARIABLE).is_some() {
        Ok(CredentialStatus::EnvironmentSecret)
    } else if let Ok(Some(_)) = store::saved(settings.store) {
        Ok(CredentialStatus::SavedSecret)
    } else if settings.use_azure_cli {
        Ok(CredentialStatus::AzureCli)
    } else {
        Ok(CredentialStatus::Missing)
    };

    Ok(Overview {
        credential,
        devices,
    })
}

pub(crate) fn save_secret(store: Store, secret: Option<&SecretString>) -> Result<()> {
    store::save(store, secret)
}

pub(crate) fn forget_device(id: &str) -> Result<()> {
    super::cleanup::forget_activity(id);
    Ok(cloud::remove(CloudProvider::Azure, id)?)
}

#[cfg(test)]
mod tests;
