//! Cloud machines handled in batches from the command line. A batch is
//! opened when the window asks the user about it, and kept, bounded, so a
//! caller can follow it with `cloud wait` until every machine in it is
//! settled. Workers update it from their own threads, so it is shared behind
//! a mutex; the window and the control server read it on the UI thread.

pub(crate) mod create;
pub(crate) mod destroy;

use crate::cloud::CloudProvider;
use crate::control::batch::{BatchInfo, BatchKind, DeviceInfo, DeviceState, Phase};
use std::{
    collections::VecDeque,
    sync::{Arc, Mutex, MutexGuard, PoisonError},
    time::{Duration, Instant},
};

/// Batches kept for `cloud wait` and `cloud batches`; the oldest go first.
const KEPT: usize = 32;
/// The longest failure kept for a device, in characters.
const ERROR_LIMIT: usize = 500;
/// How long a machine just saved still counts as being made, unless the
/// window already lists it: windows reload the device list every two
/// seconds, and in between it would count in neither place.
const HANDOFF: Duration = Duration::from_secs(30);

/// Why a machine of a batch failed, besides its provider's own errors.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Exec(#[from] crate::cloud::exec::Error),
    #[error("{0} machines cannot be created from the command line.")]
    NotOffered(&'static str),
    #[error("{}", setup_message(*.code, .output))]
    Setup { code: Option<i32>, output: String },
}

fn setup_message(code: Option<i32>, output: &str) -> String {
    let ended = match code {
        Some(code) => format!("The setup script exited {code}"), // i18n: data
        None => "A signal ended the setup script".to_owned(),    // i18n: data
    };
    if output.is_empty() {
        format!("{ended}.")
    } else {
        format!("{ended}: {output}")
    }
}

/// One saved device a batch acts on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Target {
    pub(crate) provider: CloudProvider,
    /// The machine's ID at the provider.
    pub(crate) id: String,
    /// The device's endpoint ID.
    pub(crate) endpoint: String,
    pub(crate) label: String,
}

impl Target {
    pub(crate) fn info(&self) -> DeviceInfo {
        DeviceInfo {
            provider: self.provider.key().to_owned(),
            id: self.id.clone(),
            endpoint: self.endpoint.clone(),
            label: self.label.clone(),
            state: DeviceState::Waiting,
            step: None,
            error: None,
        }
    }
}

/// Every batch the app keeps. Cloning shares it.
#[derive(Clone, Debug, Default)]
pub(crate) struct Registry(Arc<Mutex<Inner>>);

impl gpui::Global for Registry {}

#[derive(Debug, Default)]
struct Inner {
    batches: VecDeque<BatchInfo>,
    /// Endpoints of machines saved within [`HANDOFF`], oldest first.
    handoffs: VecDeque<(String, Instant)>,
    next: u64,
}

impl Inner {
    fn batch(&mut self, id: u64) -> Option<&mut BatchInfo> {
        self.batches.iter_mut().find(|batch| batch.id == id)
    }

    fn expire_handoffs(&mut self, now: Instant) {
        while self
            .handoffs
            .front()
            .is_some_and(|(_, saved)| now.duration_since(*saved) >= HANDOFF)
        {
            self.handoffs.pop_front();
        }
    }
}

impl Registry {
    fn lock(&self) -> MutexGuard<'_, Inner> {
        // A worker that panicked mid-update leaves whole values behind.
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// A new batch over `devices`, waiting for the user to accept `work`.
    /// The oldest settled batch goes once more than [`KEPT`] are kept. One
    /// still going stays, as its machines still count toward the device
    /// limit; those are bounded by that limit and the one question a window
    /// asks at a time.
    pub(crate) fn open<T>(&self, kind: BatchKind, devices: Vec<DeviceInfo>, work: T) -> Pending<T> {
        let mut inner = self.lock();
        inner.next += 1;
        let id = inner.next;
        inner.batches.push_back(BatchInfo {
            id,
            kind,
            phase: Phase::Confirming,
            devices,
        });
        while inner.batches.len() > KEPT {
            let Some(settled) = inner.batches.iter().position(|batch| batch.phase.settled()) else {
                break;
            };
            inner.batches.remove(settled);
        }
        Pending {
            registry: self.clone(),
            id,
            work: Some(work),
        }
    }

    /// A batch destroying `targets`, waiting for the user.
    pub(crate) fn open_destroy(&self, targets: &[Target]) -> Pending<Vec<Target>> {
        let devices = targets.iter().map(Target::info).collect();
        self.open(BatchKind::Destroy, devices, targets.to_vec())
    }

    pub(crate) fn get(&self, id: u64) -> Option<BatchInfo> {
        self.lock().batch(id).cloned()
    }

    pub(crate) fn all(&self) -> Vec<BatchInfo> {
        self.lock().batches.iter().cloned().collect()
    }

    /// Machines create batches are still making, or may make once the user
    /// confirms, that are not saved yet, and those just saved that `listed`
    /// does not yet know by endpoint.
    pub(crate) fn creating(&self, listed: impl Fn(&str) -> bool) -> usize {
        let mut inner = self.lock();
        inner.expire_handoffs(Instant::now());
        let making = inner
            .batches
            .iter()
            .filter(|batch| batch.kind == BatchKind::Create && !batch.phase.settled())
            .flat_map(|batch| &batch.devices)
            .filter(|device| !device.state.settled() && device.endpoint.is_empty())
            .count();
        let handed = inner
            .handoffs
            .iter()
            .filter(|(endpoint, _)| !listed(endpoint))
            .count();
        making + handed
    }

    /// Each kept batch's phase, cheaply, for the event log's every tick.
    pub(crate) fn phases(&self) -> Vec<(u64, Phase)> {
        self.lock()
            .batches
            .iter()
            .map(|batch| (batch.id, batch.phase))
            .collect()
    }

    /// Settle every device of a batch the user turned down.
    fn cancel(&self, id: u64) {
        let mut inner = self.lock();
        let Some(batch) = inner.batch(id) else {
            return;
        };
        if batch.phase != Phase::Confirming {
            return;
        }
        batch.phase = Phase::Cancelled;
        for device in &mut batch.devices {
            device.state = DeviceState::Cancelled;
        }
    }

    /// Start a batch the user accepted; whether it was still waiting.
    fn confirm(&self, id: u64) -> bool {
        let mut inner = self.lock();
        let Some(batch) = inner.batch(id) else {
            return false;
        };
        if batch.phase != Phase::Confirming {
            return false;
        }
        batch.phase = if batch.devices.is_empty() {
            Phase::Done
        } else {
            Phase::Running
        };
        true
    }

    /// Change device `index` with `change`. The batch is done once every
    /// device is settled.
    fn change(&self, id: u64, index: usize, change: impl FnOnce(&mut DeviceInfo)) {
        let mut inner = self.lock();
        let Some(batch) = inner.batch(id) else {
            return;
        };
        let Some(device) = batch.devices.get_mut(index) else {
            return;
        };
        change(device);
        if batch.phase == Phase::Running
            && batch.devices.iter().all(|device| device.state.settled())
        {
            batch.phase = Phase::Done;
        }
    }

    /// Record device `index`'s state, and why it failed.
    pub(crate) fn update(&self, id: u64, index: usize, state: DeviceState, error: Option<&str>) {
        self.change(id, index, |device| {
            device.state = state;
            device.step = None;
            device.error = error.map(|error| error.chars().take(ERROR_LIMIT).collect());
        });
    }

    /// Record what device `index` is doing now.
    pub(crate) fn progress(&self, id: u64, index: usize, state: DeviceState, step: String) {
        self.change(id, index, |device| {
            device.state = state;
            device.step = Some(step.chars().take(ERROR_LIMIT).collect());
        });
    }

    /// Name device `index` once it is made and saved.
    pub(crate) fn created(&self, id: u64, index: usize, target: &Target) {
        {
            let now = Instant::now();
            let mut inner = self.lock();
            inner.expire_handoffs(now);
            inner.handoffs.push_back((target.endpoint.clone(), now));
        }
        self.change(id, index, |device| {
            device.provider = target.provider.key().to_owned();
            device.id.clone_from(&target.id);
            device.endpoint.clone_from(&target.endpoint);
            device.label.clone_from(&target.label);
        });
    }
}

/// A batch the window is asking the user about, with the work it would
/// start. Dropping it unanswered, as closing the dialog does, cancels the
/// batch.
#[derive(Debug)]
pub(crate) struct Pending<T> {
    registry: Registry,
    id: u64,
    /// Taken once the user answers.
    work: Option<T>,
}

impl<T> Pending<T> {
    pub(crate) fn id(&self) -> u64 {
        self.id
    }

    /// What confirming starts.
    pub(crate) fn work(&self) -> Option<&T> {
        self.work.as_ref()
    }

    /// The user accepted: the registry to report to, the batch, and its
    /// work, or `None` when the batch was already settled.
    pub(crate) fn confirm(mut self) -> Option<(Registry, u64, T)> {
        let work = self.work.take()?;
        self.registry
            .confirm(self.id)
            .then(|| (self.registry.clone(), self.id, work))
    }
}

impl<T> Drop for Pending<T> {
    fn drop(&mut self) {
        if self.work.is_some() {
            self.registry.cancel(self.id);
        }
    }
}

#[cfg(test)]
mod tests;
