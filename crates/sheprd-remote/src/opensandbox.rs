//! OpenSandbox, a `cloud` provider for a self-hosted sandbox server: its
//! sandboxes become devices. An API key against the configured server makes,
//! resumes, and deletes them over bounded REST calls, and this app runs
//! itself as a relay (see [`relay`]) that turns the sandbox's execd stream
//! into the standard streams Herdr's bridge, installer, and Teleport's
//! scripts already use. Everything here blocks, so it runs on background
//! workers.

mod api;
mod connect;
mod error;
mod http;
pub mod relay;
mod settings;
pub mod setup;
mod store;

pub use error::{Error, Status};
pub use {
    connect::{command, connect},
    settings::{DEFAULT_API_URL, Settings},
    setup::destroy_saved,
};

pub type Result<T, E = Error> = std::result::Result<T, E>;

#[cfg(test)]
mod tests;
