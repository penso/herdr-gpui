//! Destroying the devices of a confirmed batch, each on its own thread: a
//! provider's delete waits on the provider for minutes, and one slow machine
//! must not hold up the rest. Each thread reports to the registry itself, so
//! the work finishes and is recorded even if the window that asked closes.

use super::{Registry, Target};
use crate::control::batch::DeviceState;
use std::sync::Arc;

/// Start destroying every target of batch `batch`. `run` does one device
/// and says how it ended; it blocks, off the UI thread.
pub(crate) fn start(
    registry: &Registry,
    batch: u64,
    targets: Vec<Target>,
    run: impl Fn(&Target) -> crate::Result<DeviceState> + Send + Sync + 'static,
) {
    let run = Arc::new(run);
    for (index, target) in targets.into_iter().enumerate() {
        registry.update(batch, index, DeviceState::Destroying, None);
        let worker = registry.clone();
        let run = run.clone();
        let spawned = std::thread::Builder::new()
            .name("herdr-cloud-destroy".into())
            .spawn(move || {
                let result = run(&target);
                if let Err(error) = &result {
                    tracing::warn!(
                        category = "cloud_batch",
                        batch,
                        endpoint = target.endpoint,
                        %error,
                        "Could not destroy a cloud device"
                    );
                }
                match result {
                    Ok(state) => worker.update(batch, index, state, None),
                    Err(error) => {
                        worker.update(batch, index, DeviceState::Failed, Some(&error.to_string()))
                    }
                }
            });
        if let Err(error) = spawned {
            tracing::error!(category = "cloud_batch", %error, "Could not start a destroy worker");
            registry.update(
                batch,
                index,
                DeviceState::Failed,
                Some("Herdr UI could not start a worker for it"), // i18n: data
            );
        }
    }
}

/// Delete the device at its provider and forget it, or, for a provider that
/// cannot delete a machine from here, only forget it. Blocks.
pub(crate) fn destroy(target: &Target) -> crate::Result<DeviceState> {
    match crate::cloud::deletion(target.provider) {
        Some(deletion) => {
            (deletion.run)(&target.id)?;
            Ok(DeviceState::Destroyed)
        }
        None => {
            crate::cloud::remove(target.provider, &target.id)?;
            Ok(DeviceState::Forgotten)
        }
    }
}
