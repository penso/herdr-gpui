//! Railway's free VMs as `cloud` devices. `ssh railway.new` needs no account
//! or token: the SSH key you present is the identity, and the same key reaches
//! the same VM. So a Railway device is an SSH key, either one the user already
//! has or one this app generated, whose path is saved as the device's account
//! (never a copy of the key) and used for setup and every reconnect.
//!
//! Setup is the only place that creates the VM (a one-off session on a
//! pseudo-terminal, see `first_use`) or installs Herdr (after approval).
//! Reconnects do neither: they check the key and run Herdr's bridge. Everything
//! here blocks, so it runs on background workers.

mod connect;
pub mod create;
mod error;
mod first_use;
mod keys;
mod refusal;
pub mod setup;
mod ssh;

pub use connect::{command, connect};
pub use error::Error;
pub use keys::{KeyDirectory, NAME_RULE, valid_name};

pub type Result<T, E = Error> = std::result::Result<T, E>;

// Shared by the tests; they drive `ssh`, `ssh-keygen`, and `/bin/sh`, and cloud
// devices are Unix-only.
#[cfg(all(test, unix))]
mod tests;
