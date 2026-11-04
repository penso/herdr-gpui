//! Intentional stops are shared by every window and connection worker. Only
//! an explicit reconnect clears them. Engine operations serialize per device
//! so a start already in flight cannot finish after a successful stop, and a
//! start waits for every stop asked for before it.

use super::{Engine, Error, Result};
use sheprd_client::{CloudProvider, ConnectTarget};
use std::{
    collections::HashMap,
    sync::{Arc, Condvar, LazyLock, Mutex, MutexGuard, PoisonError},
};

#[derive(Default)]
struct Lifecycle {
    // Never hold this lock while running an engine command: the UI reads it.
    state: Mutex<State>,
    // Signalled whenever a stop finishes, for starts waiting on it.
    settled: Condvar,
    operation: Mutex<()>,
}

/// A device's stop intent. A reconnect begins a new epoch, so a stop asked
/// for before it cannot suspend the device again, and one stop failing
/// cannot undo another that succeeded.
#[derive(Default)]
struct State {
    epoch: u64,
    /// A stop in this epoch succeeded.
    stopped: bool,
    /// Stops still running, from any epoch: a start waits for all of them.
    pending: usize,
    /// Those asked for in this epoch, which already count as a stop.
    pending_now: usize,
}

impl State {
    fn suspended(&self) -> bool {
        self.stopped || self.pending_now > 0
    }
}

type Devices = HashMap<String, HashMap<String, Arc<Lifecycle>>>;
static DEVICES: LazyLock<Mutex<Devices>> = LazyLock::new(Mutex::default);

fn device(account: &str, id: &str) -> Arc<Lifecycle> {
    DEVICES
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .entry(account.to_owned())
        .or_default()
        .entry(id.to_owned())
        .or_default()
        .clone()
}

fn target_device(target: &ConnectTarget) -> Option<Arc<Lifecycle>> {
    let ConnectTarget::Cloud {
        provider: CloudProvider::Container,
        account,
        id,
        ..
    } = target
    else {
        return None;
    };
    DEVICES
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .get(account)?
        .get(id)
        .cloned()
}

pub fn suspended(target: &ConnectTarget) -> bool {
    target_device(target).is_some_and(|device| device.is_suspended())
}

/// UI-safe: releases the stop intent without waiting for the engine. The next
/// connection worker waits for any pending stop before starting the container.
pub fn resume(target: &ConnectTarget) {
    if let Some(device) = target_device(target) {
        device.resume();
    }
}

/// Release the stop intent on the container `id`, as a reconnect does, for a
/// device the user adds back.
pub(super) fn resume_device(engine: Engine, id: &str) {
    device(engine.key(), id).resume();
}

pub(super) fn start(engine: Engine, id: &str, start: impl FnOnce() -> Result<()>) -> Result<()> {
    device(engine.key(), id).start(id, start)
}

pub fn stop(engine: Engine, id: &str, stop: impl FnOnce() -> Result<()>) -> Result<()> {
    device(engine.key(), id).stop(stop)
}

impl Lifecycle {
    fn state(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn is_suspended(&self) -> bool {
        self.state().suspended()
    }

    fn resume(&self) {
        let mut state = self.state();
        state.epoch += 1;
        state.stopped = false;
        state.pending_now = 0;
    }

    /// Run `start` unless the device is suspended, after every stop asked for
    /// so far, even one still queued behind an earlier start.
    fn start(&self, id: &str, start: impl FnOnce() -> Result<()>) -> Result<()> {
        loop {
            let mut state = self.state();
            while state.pending > 0 {
                state = self
                    .settled
                    .wait(state)
                    .unwrap_or_else(PoisonError::into_inner);
            }
            drop(state);
            let _operation = self
                .operation
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            let state = self.state();
            // A stop asked for meanwhile is waiting for this lock: let it run.
            if state.pending > 0 {
                continue;
            }
            if state.suspended() {
                return Err(Error::Suspended(id.to_owned()));
            }
            drop(state);
            return start();
        }
    }

    fn stop(&self, stop: impl FnOnce() -> Result<()>) -> Result<()> {
        let epoch = {
            let mut state = self.state();
            state.pending += 1;
            state.pending_now += 1;
            state.epoch
        };
        let result = {
            let _operation = self
                .operation
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            stop()
        };
        let mut state = self.state();
        state.pending -= 1;
        if state.epoch == epoch {
            state.pending_now -= 1;
            state.stopped |= result.is_ok();
        }
        drop(state);
        self.settled.notify_all();
        result
    }
}

#[cfg(test)]
mod tests;
