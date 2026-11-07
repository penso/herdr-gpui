//! boxd, a `cloud` provider whose machines become devices: full Linux
//! microVMs with a persistent disk that start in about a second and go to
//! standby or hibernate when idle. An API key against the configured API
//! origin makes, wakes, and deletes them over bounded grpc-web calls, and
//! this app runs itself as a relay (see [`relay`]) that turns a machine's
//! exec stream into the standard streams Herdr's bridge, installer, and
//! Teleport's scripts already use. Everything here blocks, so it runs on
//! background workers.

mod api;
mod connect;
mod error;
mod http;
pub mod relay;
mod settings;
pub mod setup;
mod store;
mod stream;
mod wire;

pub use error::{Code, Error, Status};
pub use {
    connect::{command, connect},
    settings::{DEFAULT_API_URL, Settings},
    setup::destroy_saved,
};

pub type Result<T, E = Error> = std::result::Result<T, E>;

#[cfg(test)]
mod tests;
