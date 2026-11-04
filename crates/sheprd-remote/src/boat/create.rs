//! Making a Boat sandbox on demand, as `cloud create` asks. A creation is
//! journalled per account until Boat names the sandbox, so creations run one
//! at a time, as Settings runs them.

use super::setup::{self, AddRequest, Target};
use crate::cloud::{self, SavedDevice, Spec};
use std::sync::{Mutex, PoisonError};

static CREATING: Mutex<()> = Mutex::new(());

/// Make the sandbox `spec` describes and save it as a device. Blocks, also
/// while another creation runs.
pub fn provision(
    spec: &Spec,
    cancelled: &dyn Fn() -> bool,
    report: &dyn Fn(cloud::Step),
) -> crate::Result<SavedDevice> {
    let settings = crate::config::load()?
        .boat
        .settings()?
        .ok_or(super::Error::Account)?;
    let _turn = CREATING.lock().unwrap_or_else(PoisonError::into_inner);
    setup::add_device(
        AddRequest {
            settings,
            target: Target::Create(spec.name.clone().unwrap_or_else(setup::random_name)),
            install: spec.install,
        },
        cancelled,
        report,
    )
}
