//! Boat, a `cloud` provider whose sandboxes become devices: an API key against
//! a configured API root, bounded REST calls, sandbox readiness, and pinned SSH
//! using a GUI-owned key. Everything here blocks, so it runs on background
//! workers and reports back through the window's mailboxes.

mod api;
mod connect;
pub mod create;
pub mod destroy;
mod error;
mod gateway;
mod http;
mod pending;
mod settings;
pub mod setup;
mod store;

pub use error::{Error, Status};
pub use {
    api::SandboxCount,
    connect::{command, connect},
    settings::{DEFAULT_API_URL, Settings},
};

pub type Result<T, E = Error> = std::result::Result<T, E>;

#[cfg(test)]
pub(crate) mod tests;
