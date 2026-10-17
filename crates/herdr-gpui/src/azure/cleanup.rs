//! Idle tracking and automatic cleanup for Azure devices.
//!
//! Tracks last activity times on Azure VM devices. A VM idle (or disconnected)
//! for longer than the configured `auto_stop` is destroyed when it is
//! disposable, or deallocated when it is an older VM on a managed disk, so it
//! stops billing. A device whose machine Azure no longer has is forgotten once
//! nothing it billed for is left.

use super::{
    Result, Settings, api,
    http::{Audience, Client},
    machine::Machine,
};
use crate::cloud::{CloudProvider, SavedDevice};
use std::{
    collections::HashMap,
    sync::Mutex,
    time::{Duration, Instant},
};

static LAST_ACTIVITY: Mutex<Option<HashMap<String, Instant>>> = Mutex::new(None);

/// Record recent activity on an Azure VM.
pub(crate) fn record_activity(vm_id: &str) {
    if let Ok(mut guard) = LAST_ACTIVITY.lock() {
        let map = guard.get_or_insert_with(HashMap::new);
        map.insert(vm_id.to_owned(), Instant::now());
    }
}

/// Check if an Azure VM has been idle for at least `timeout`. A VM this
/// process has not seen yet starts its timeout now: the map is empty at
/// launch and a new VM is saved before it connects, so neither counts as
/// idle until a full `timeout` passes without a connection.
pub(crate) fn is_idle(vm_id: &str, timeout: Duration) -> bool {
    let Ok(mut guard) = LAST_ACTIVITY.lock() else {
        return false;
    };
    let last = guard
        .get_or_insert_with(HashMap::new)
        .entry(vm_id.to_owned())
        .or_insert_with(Instant::now);
    last.elapsed() >= timeout
}

/// Reset activity tracking for a VM (e.g. after deallocating or deleting).
pub(crate) fn forget_activity(vm_id: &str) {
    if let Ok(mut guard) = LAST_ACTIVITY.lock()
        && let Some(map) = guard.as_mut()
    {
        map.remove(vm_id);
    }
}

/// Deallocate a kept VM that is running. Returns whether it was running.
fn deallocate(client: &Client, vm_name: &str) -> Result<bool> {
    if api::get_vm_power_state(client, vm_name)? != api::PowerState::Running {
        return Ok(false);
    }
    tracing::info!(
        category = "azure_cleanup",
        vm_name,
        "Deallocating idle Azure VM to prevent ongoing compute charges"
    );
    api::deallocate_vm(client, vm_name)?;
    forget_activity(vm_name);
    Ok(true)
}

/// The saved Azure devices of `account`. A device saved under another
/// subscription is not this client's to check: a machine of the same name
/// here would be stopped or forgotten instead.
fn account_devices(devices: Vec<SavedDevice>, settings: &Settings) -> Vec<SavedDevice> {
    devices
        .into_iter()
        .filter(|device| {
            device.provider == CloudProvider::Azure
                && settings.owns(&device.account)
                && !super::destroy::destroying(&device.id)
        })
        .collect()
}

/// Forget a sandbox Azure no longer has: one deleted in the portal. Its row
/// would only fail to connect, and a sandbox leaves nothing else billing.
fn forget_gone(device: &SavedDevice) -> crate::Result<()> {
    tracing::info!(
        category = "azure_cleanup",
        id = %device.id,
        "Forgetting an Azure device whose machine is gone"
    );
    forget_activity(&device.id);
    Ok(crate::cloud::remove(CloudProvider::Azure, &device.id)?)
}

/// Each minute: forget devices whose machine is gone, and stop idle VMs once
/// `auto_stop` has passed without a connection. A disposable VM cannot be
/// stopped and kept, so it is destroyed with everything it bills for; an
/// older VM on a managed disk is deallocated. Sandboxes suspend themselves.
pub(crate) fn check_all_idle_devices() {
    let Ok(devices) = crate::cloud::load() else {
        return;
    };
    if !devices
        .iter()
        .any(|device| device.provider == CloudProvider::Azure)
    {
        return;
    }
    let Ok(config) = crate::config::Config::load_startup() else {
        return;
    };
    let Ok(Some(settings)) = config.azure.settings() else {
        return;
    };
    let devices = account_devices(devices, &settings);
    let (vms, sandboxes): (Vec<_>, Vec<_>) = devices
        .iter()
        .partition(|device| matches!(Machine::parse(&device.id), Machine::Vm(_)));
    if !vms.is_empty() {
        match Client::new(&settings) {
            Ok(client) => {
                for device in vms {
                    if let Err(error) = check_vm(&settings, &client, device) {
                        warn(device, &error);
                    }
                }
            }
            Err(error) => {
                tracing::warn!(category = "azure_cleanup", %error, "Could not reach Azure")
            }
        }
    }
    if sandboxes.is_empty() {
        return;
    }
    let client = match Client::for_audience(&settings, Audience::Sandboxes) {
        Ok(client) => client,
        Err(error) => {
            return tracing::warn!(category = "azure_cleanup", %error, "Could not reach Azure sandboxes");
        }
    };
    for device in sandboxes {
        let Machine::Sandbox(id) = Machine::parse(&device.id) else {
            continue;
        };
        let checked = super::sandbox::exists(&client, &settings, id)
            .map_err(crate::Error::from)
            .and_then(|exists| if exists { Ok(()) } else { forget_gone(device) });
        if let Err(error) = checked {
            warn(device, &error);
        }
    }
}

/// What the minute's check does to a saved VM.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum VmAction {
    Leave,
    /// The VM is gone, but its network or disk may not be: an eviction, a
    /// portal delete, or a destroy that failed partway. Delete what is left
    /// and forget the device only once Azure has none of it, so a failure
    /// keeps the device for the next minute to retry.
    FinishDelete,
    Destroy,
    Deallocate,
}

fn vm_action(
    kind: Option<api::VmKind>,
    auto_stop: Option<Duration>,
    idle: impl FnOnce(Duration) -> bool,
) -> VmAction {
    let Some(kind) = kind else {
        return VmAction::FinishDelete;
    };
    match auto_stop {
        Some(auto_stop) if idle(auto_stop) => match kind {
            api::VmKind::Disposable => VmAction::Destroy,
            api::VmKind::Kept => VmAction::Deallocate,
        },
        _ => VmAction::Leave,
    }
}

fn check_vm(settings: &Settings, client: &Client, device: &SavedDevice) -> crate::Result<()> {
    let Machine::Vm(name) = Machine::parse(&device.id) else {
        return Ok(());
    };
    let kind = api::vm_kind(client, name)?;
    match vm_action(kind, settings.auto_stop, |timeout| {
        is_idle(&device.id, timeout)
    }) {
        VmAction::Leave => {}
        VmAction::FinishDelete => {
            tracing::info!(
                category = "azure_cleanup",
                vm_name = name,
                "Deleting what is left of a gone Azure VM before forgetting it"
            );
            super::destroy::destroy_device(settings, device, &|| false, |_| {})?;
        }
        VmAction::Destroy => {
            tracing::info!(
                category = "azure_cleanup",
                vm_name = name,
                "Destroying an idle disposable Azure VM"
            );
            super::destroy::destroy_device(settings, device, &|| false, |_| {})?;
        }
        VmAction::Deallocate => {
            deallocate(client, name)?;
        }
    }
    Ok(())
}

fn warn(device: &SavedDevice, error: &crate::Error) {
    tracing::warn!(
        category = "azure_cleanup",
        id = %device.id,
        %error,
        "Failed checking an Azure device"
    );
}

/// Check Azure devices each minute in the background; see `check_all_idle_devices`.
pub(crate) fn install(cx: &mut gpui::App) {
    let timer = cx.background_executor().clone();
    cx.background_executor()
        .spawn(async move {
            loop {
                timer.timer(Duration::from_secs(60)).await;
                check_all_idle_devices();
            }
        })
        .detach();
}

#[cfg(test)]
mod tests;
