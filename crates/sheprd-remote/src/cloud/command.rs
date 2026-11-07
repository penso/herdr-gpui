//! Prepare the same authenticated command used for a device connection, keeping
//! provider-owned credentials alive while a background operation runs.

use super::CloudProvider;
use std::{
    io,
    process::Command,
    sync::atomic::{AtomicBool, Ordering},
};

pub fn with_command<T>(
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
    super::providers::run_command(provider, account, id, cancelled, run)
}

#[cfg(test)]
mod tests;
