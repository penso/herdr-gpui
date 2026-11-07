//! Making a Railway VM on demand, as `cloud create` asks. Railway gives each
//! SSH key one VM, so every new VM gets a key of its own; VMs are set up one
//! at a time, as Settings sets them up.

use super::setup::{self, AddRequest};
use crate::cloud::{self, SavedDevice, Spec};
use std::sync::{Mutex, PoisonError};

static CREATING: Mutex<()> = Mutex::new(());

/// Make the VM `spec` describes and save it as a device. Blocks, also while
/// another VM is set up.
pub fn provision(
    spec: &Spec,
    cancelled: &dyn Fn() -> bool,
    report: &dyn Fn(cloud::Step),
) -> crate::Result<SavedDevice> {
    let name = spec.name.clone().unwrap_or_else(setup::random_name);
    let _turn = CREATING.lock().unwrap_or_else(PoisonError::into_inner);
    let key = setup::generate_key(&name, cancelled)?;
    setup::add_device(
        AddRequest {
            key,
            name,
            install: spec.install,
        },
        cancelled,
        report,
    )
}
