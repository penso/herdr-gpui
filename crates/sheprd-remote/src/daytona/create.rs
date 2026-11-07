//! Making a Daytona sandbox on demand, as `cloud create` asks, from a
//! snapshot when one is given.

use super::setup::{self, AddRequest};
use crate::cloud::{self, SavedDevice, Spec};

/// Make the sandbox `spec` describes and save it as a device. Blocks.
pub fn provision(
    spec: &Spec,
    cancelled: &dyn Fn() -> bool,
    report: &dyn Fn(cloud::Step),
) -> crate::Result<SavedDevice> {
    let mut settings = crate::config::load()?
        .daytona
        .settings()?
        .ok_or(super::Error::Account)?;
    if let Some(image) = &spec.image {
        settings.snapshot = Some(image.clone());
    }
    setup::add_device(
        AddRequest {
            settings,
            name: spec.name.clone().unwrap_or_else(setup::random_name),
            install: spec.install,
        },
        cancelled,
        report,
    )
}
