//! Making an Azure VM or Container Apps sandbox on demand, as `cloud create`
//! asks, with the configured account.

use super::setup::{self, AddRequest, Target};
use crate::cloud::{self, SavedDevice, Shape, Spec};

/// Make the machine `spec` describes and save it as a device. Blocks.
pub fn provision(
    spec: &Spec,
    cancelled: &dyn Fn() -> bool,
    report: &dyn Fn(cloud::Step),
) -> crate::Result<SavedDevice> {
    let settings = crate::config::load()?
        .azure
        .settings()?
        .ok_or(super::Error::Account)?;
    let name = spec.name.clone().unwrap_or_else(setup::random_name);
    match spec.shape {
        Shape::Machine => setup::add_device(
            AddRequest {
                settings,
                target: Target::Create(name),
                install: spec.install,
            },
            cancelled,
            report,
        ),
        Shape::Sandbox => {
            super::sandbox::add_device(settings, name, spec.install, cancelled, report)
        }
    }
}
