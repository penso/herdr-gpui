//! Prepare the same authenticated command used for a device connection, keeping
//! provider-owned credentials alive while a background operation runs.

use super::CloudProvider;
use std::{
    io,
    process::Command,
    sync::atomic::{AtomicBool, Ordering},
};

pub(crate) fn with_command<T>(
    provider: CloudProvider,
    account: &str,
    id: &str,
    cancelled: &AtomicBool,
    run: impl FnOnce(Command) -> T,
) -> io::Result<T> {
    if cancelled.load(Ordering::Acquire) {
        return Err(io::Error::new(
            io::ErrorKind::Interrupted,
            super::Error::Cancelled,
        ));
    }
    match provider {
        #[cfg(feature = "azure")]
        CloudProvider::Azure => {
            let command =
                crate::azure::command(account, id, cancelled).map_err(io::Error::other)?;
            Ok(run(command))
        }
        #[cfg(feature = "boat")]
        CloudProvider::Boat => {
            let gateway = crate::boat::command(account, id, cancelled).map_err(io::Error::other)?;
            Ok(gateway.with_command(run))
        }
        #[cfg(feature = "coder")]
        CloudProvider::Coder => {
            let command =
                crate::coder::command(account, id, cancelled).map_err(io::Error::other)?;
            Ok(run(command))
        }
        #[cfg(feature = "daytona")]
        CloudProvider::Daytona => {
            let gateway =
                crate::daytona::command(account, id, cancelled).map_err(io::Error::other)?;
            Ok(gateway.with_command(run))
        }
        #[cfg(feature = "railway")]
        CloudProvider::Railway => {
            let command =
                crate::railway::command(account, id, cancelled).map_err(io::Error::other)?;
            Ok(run(command))
        }
    }
}

#[cfg(test)]
mod tests;
