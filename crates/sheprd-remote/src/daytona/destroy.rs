//! Deleting a saved device's sandbox at Daytona, then forgetting the device.
//! The device stays saved until Daytona accepts the delete, so a failure can
//! be retried.

use super::{Error, Settings, http::Request, store};
use crate::cloud::{self, CloudProvider};

/// Delete the saved device `id`'s sandbox with the configured account, then
/// forget it. A device no longer saved has nothing left to do; one saved
/// under another account is refused rather than deleted with this key.
/// Blocks: run it off the UI thread.
pub fn destroy_saved(id: &str) -> crate::Result<()> {
    let Some(device) = cloud::load()?
        .into_iter()
        .find(|device| device.provider == CloudProvider::Daytona && device.id == id)
    else {
        return Ok(());
    };
    let settings = crate::config::load()?
        .daytona
        .settings()?
        .ok_or(Error::Account)?;
    Ok(destroy(&settings, &device.account, id)?)
}

pub(super) fn destroy(settings: &Settings, account: &str, id: &str) -> super::Result<()> {
    if settings.base != account {
        return Err(Error::Account);
    }
    let key = store::key(settings)?;
    Request {
        settings,
        key: &key,
    }
    .destroy(id)?;
    Ok(cloud::remove(CloudProvider::Daytona, id)?)
}

#[cfg(test)]
mod tests;
