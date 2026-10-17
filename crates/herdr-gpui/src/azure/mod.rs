//! Azure Virtual Machines and Container Apps sandboxes as Herdr cloud devices.

pub(crate) mod api;
pub(crate) mod cleanup;
pub(crate) mod cli;
pub(crate) mod connect;
pub(crate) mod destroy;
pub(crate) mod error;
pub(crate) mod gateway;
pub(crate) mod http;
pub(crate) mod machine;
pub(crate) mod sandbox;
pub(crate) mod settings;
pub(crate) mod setup;
pub(crate) mod sizes;
pub(crate) mod store;

pub(crate) use cleanup::{forget_activity, install, record_activity};
pub(crate) use connect::{command, connect};
// Monitors sample through a remote shell, which only Unix clients start.
#[cfg(unix)]
pub(crate) use connect::monitor_command;
pub(crate) use error::{Error, Result};
pub(crate) use settings::Settings;

#[cfg(test)]
mod tests;
