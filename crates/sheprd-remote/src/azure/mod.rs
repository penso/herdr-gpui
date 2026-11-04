//! Azure Virtual Machines and Container Apps sandboxes as Herdr cloud devices.

pub mod api;
pub mod cleanup;
pub mod cli;
pub mod connect;
pub mod create;
pub mod destroy;
pub mod error;
pub mod gateway;
pub mod http;
pub mod machine;
pub mod sandbox;
pub mod settings;
pub mod setup;
pub mod sizes;
pub mod store;

pub use cleanup::check_all_idle_devices;
pub use cleanup::{forget_activity, record_activity};
pub use connect::{command, connect};
// Monitors sample through a remote shell, which only Unix clients start.
#[cfg(unix)]
pub use connect::monitor_command;
pub use error::{Error, Result};
pub use settings::Settings;

#[cfg(test)]
mod tests;
